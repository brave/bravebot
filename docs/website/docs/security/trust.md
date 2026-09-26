---
sidebar_position: 1
title: Trusted directories
description: The question at startup, every other way a path comes to be trusted, and how long an answer lasts.
---

# Trusted directories

At startup you are asked whether you trust the working directory.

- **Trust it** and a rule covering the whole tree is written, so ordinary work proceeds without a
  prompt for every edit.
- **Decline** and nothing is written, so nothing is trusted and every write is shown to you first.
- **Trust and remember** (`r`, in the terminal interface) trusts it as a yes does and writes the
  answer down, so later sessions started in exactly that directory are not asked. See
  [Remembering the answer](#remembering-the-answer).
- **Leaving at the question starts no session.**

That record is the **trust map**, and it is the thing every read and every write consults.

A session started with `--dangerously-skip-permissions` is the one exception: the question is not put
at all, and the map is the one a yes would have written. That mode already approves vouching for
every unvouched file the planner reads, so the tree becomes trusted a file at a time either way. A
resume still takes the map from its own record even there. See
[modes](permissions.md#answering-in-advance-modes).

## Nothing is trusted until it is granted

An empty map trusts no path. Trust is granted by a person, and never inferred from silence, from a
path's shape, or from anything a model or a file said. That is what makes declining at startup mean
something.

## How a path is matched

Rules are keyed by path prefix and matched by whole segments, and **the longest matching prefix
decides**. Both polarities are expressible, so a trusted tree may hold an untrusted subtree, which may
hold a trusted path again. Equivalent spellings of a path are one rule, and a later decision replaces
an earlier one.

That is what lets `@vendor/lib.js` be trusted inside a `vendor` you marked untrusted, without the
answer leaking to its siblings.

**Every rule is keyed on a full path, and there is one set of them.** You may name a file
relatively, which is how a file in the project is named, and the map reads that under the working
directory: `src/main.rs` in a session working in `/work` is a rule about `/work/src/main.rs`, and the
project's own rule is `/work`. So the project is a path like any other rather than a prefix covering
everything, and a directory you opened by name sits in the same map without reaching into the
project: `~/notes` does not prefix `~/proj`, and whole-segment matching is what stops a rule about
`~/proj` covering `~/proj-secret`.

A rule about a directory that *does* hold the project covers the project's files, because you
vouched for a tree the project is in. The project's own rules are the more specific ones and still
decide wherever they exist, so a no given inside the project is not undone by a yes given above it.

A rule is about a **path**, not about the files that were in it when the rule was made, and it is
consulted when a file is read rather than when the rule is written. A file that appears in a trusted
directory afterwards is therefore read as trusted, whoever put it there.

### Which name a path is asked about under

A path is reduced to the open directory it lands in, and takes that directory's recorded name with
the rest of the path as it was spelled. The open directories are the working directory and the ones
you opened by name, each under the path its name resolved to. So a file in the project reaches one
rule whichever way it is spelled: naming it in full finds the rule its relative name wrote, and
vouching for it under either spelling records the same rule.

This is what makes two spellings of a directory one rule, including the spellings only the
filesystem can tell apart. A directory named through a link is the same directory, which matters on
macOS where `/tmp` and `$TMPDIR` are both links: without this, a file you vouched for would be
quarantined under half its names.

Two paths are asked about exactly as written, because neither has a spelling under a recorded name:
one landing in no open directory, and one reaching an open directory through a link straight into the
middle of it rather than through that directory's own name. Nothing covers either.

**A directory whose resolved name cannot be keyed is refused rather than opened.** A key is spelled
from `/`, so `/add-dir` and `/cd` both refuse a path that is not, rather than recording a rule under
a name that would be read as a path inside the project, where your answer at startup covers it. On
Windows that is every path there is, so opening a directory by name is unavailable there for now.

## What a write does

Every row here is exact. A write matching a row does what that row says and nothing else.

| data | destination | prompt? | effect on the map |
|---|---|---|---|
| trusted | trusted | no | unchanged |
| untrusted | trusted | **yes** | that path becomes untrusted |
| trusted | untrusted | no | that path becomes trusted |
| untrusted | untrusted | no | unchanged |
| either | never mentioned | **yes** | that path takes the data's trust |

A prompt here asks one question and only this one: **may this path stop being trusted?** That is the
only consequence a later step cannot undo, since a path recorded as untrusted can no longer be
examined or edited.

- **Writing trusted data never asks.** Trusted data means the turn observed nothing untrusted, so it
  holds no byte an attacker influenced, and the destination only ever gains trust.
- **Untrusted data into a trusted path must mark it untrusted.** This closes the round trip: written
  into a trusted tree and read back as trusted, untrusted bytes would launder injected text into
  trusted input, and the map would become a bypass for the gate it exists to support.
- **A path nobody has mentioned asks either way**, because there is no decision behind it yet, and the
  first write there is the moment to ask.

Reconciliation marks the exact path written, never the parent. One untrusted file does not taint its
siblings, and marking the parent would turn a single fetched page into a project nobody may edit.

## Every way a rule gets written

Each grants exactly one thing, and grants it because a person made a gesture, never because anything
inspected content.

| Gesture | What it grants |
|---|---|
| yes at the startup question | the whole working directory, for this session |
| `r` at the startup question | the whole working directory, for this session and every later one started in exactly that directory, until [`/forget-trust`](../reference/commands.md#forget-trust) |
| [`@path`](../using/context.md#naming-a-file-path) or `--file` | that one file, for the rest of the session |
| [dropping a file](../using/context.md#dropping-a-file) | that one file, wherever on disk it is, plus reach to it |
| `/add-dir <path>` | that directory: reachable **and** trusted, for this session |
| `/cd <path>` | that directory, as the new working directory, for this session |
| yes at a quarantined read | that one path, for the rest of the session |
| yes to a directory your settings file named | that directory: reachable **and** trusted, for this session |

Each name in `permissions.additionalDirectories` in your
[settings file](../customize/configuration.md#permissions) is put to you as its own question when the
session opens. Accept one and it is opened by the route `/add-dir` takes and trusted on the same
terms, which is what keeps a directory you named in a file and one you typed from being reachable on
different terms. Decline and it is neither reachable nor trusted. Leaving at any of the questions
starts no session.

The question shows the path the name resolved to rather than the name as it was written, because
opening a directory follows a link wherever it leads: a box showing the spelling would collect an
answer about a different tree. A name that cannot be opened is reported rather than asked about, and
a directory two layers both named is one question.

A settings file names a destination and grants nothing, which is why each name is a question. The
layers a checkout carries arrive with the checkout, so a name that opened a directory by itself would
be reach and trust granted by whoever last edited the file. The cost is that a file naming thirty
directories is thirty boxes before you can type anything, and Ctrl-C is the way out of a list you do
not want to answer.

A session resumed with its own map is asked none of these and opens none of them, since the
directories it has open are the ones its own record reopened. `/clear` closes the ones that were open
and opens none.

### The quarantined-read prompt

When a turn reads a file nobody has vouched for, you are shown the path and the first lines of it and
asked whether to trust it:

```
╭ let the model read this file? ────────────────────────────╮
│Trust game.js                                              │
│                                                           │
│  the check found no attempt to give instructions in this  │
│                                                           │
│  the model cannot read this file, so it is working blind  │
│  on it. Vouching lets it read this file for the rest of   │
│  this session, here and in every later read.              │
│                                                           │
│┃ const SPEED = 100;                                       │
│                                                           │
│  y trust it    n leave it quarantined    ctrl-c stop      │
╰───────────────────────────────────────────────────────────╯
```

Yes writes exactly the rule `@` would have written, so it stays consistent for every later read. It is
asked once per path per turn, and only where the read is quarantined. Declining leaves the file as it
was and the turn carries on with a reference.

**The line above the preview is a [check](vetting.md) reading the whole file**, not the few lines you
can see, because a yes grants the whole file. It decides nothing: a yes writes the rule whatever the
check said, and a no writes nothing whatever it said. It is there because the preview alone left you
answering the question that grants the most with the least in front of you.

**What the file holds decides nothing about whether you are asked.** A file with nothing to show,
because it is empty or does not read as text, is asked about like any other, and the prompt says so
where the preview would be.

**What does decide it is the path, and only a path naming a file is offered.** Never a picture, because
a yes grants that a file's *text* may be read and a picture's text is never read whatever the map says.
Never a directory, and never a path naming nothing: a yes writes a rule covering everything beneath the
name it was given, so a box titled with one file would hand over every file beneath the name at once,
over a string the planner chose rather than one you typed.

### `/add-dir`

```
/add-dir ~/notes
```

records a rule under that directory's own path that does two things together: the directory becomes
reachable, since an absolute path is otherwise refused whatever the map says, and it is recorded as
trusted. Either half alone is no use. One leaves a rule about files nothing can open, the other a
directory that prompts on every edit.

It lasts the session, `--resume` carries both halves, and `/clear` closes it. A directory already
inside the project is refused. A directory a resume cannot open again, because it has moved or gone,
says so rather than being passed over.

A directory that *holds* the project is **not** refused, and its rule covers the project's files as
it covers everything else in that tree. Vouching for a directory is a standing statement about the
place, and the project is in it, so there is nothing to except.

**The command-line [`--add-dir`](../reference/cli.md#--add-dir-path) grants only the first half.** A
one-shot run can reach the directory and vouches for nothing in it, since the gesture behind the
trusted half is a person typing the path in a session where they have already answered for the
directory they are working in, and a run nobody is watching has answered nothing.

## Moving the working directory

```
/cd ~/projects/other
```

makes that directory the working directory and vouches for it, on the same terms `/add-dir` uses: you
typed the path, and a later decision replaces an earlier one. See
[`/cd`](../reference/commands.md#cd-path) for everything else it moves.

**The map says what it always said, and carries nothing with it.** Every rule names the file it
always named, because every key is a full path. What changes is only which rules a relative name
reaches and how each is spelled back to you. So a yes given for one project does not become a yes for
another, and a no given inside the old one is not forgotten. That grants nothing and withdraws
nothing, which is what makes it something bravebot can do without asking you.

Anything overlapping the new working directory closes: the directory you left, and any `/add-dir`
directory holding it or sitting inside it, each said out loud as it happens. A directory reachable
through the new root as well as under its own name would leave two names for a path to be asked about
under, with nothing to choose between them.

## Reach stays confined

No rule extends reach. Reading, writing, editing, listing and searching are confined to the working
directory and to whatever has been opened beside it, the [directory the session was
given](#a-directory-of-the-sessions-own) among them. `..` and absolute paths outside those are refused
rather than resolved, in an added directory exactly as in the project, and a symlink leaving one is
refused. A relative path always means the project, never a directory opened beside it.

Confinement is about where an operation lands rather than how its path is spelled, so it holds for a
file that does not exist yet: a write creates what it names, and a symlink out of the tree is refused
whether or not there is anything at the other end of it.

## A directory of the session's own

Every session is given a directory of its own in the system temporary directory, created as the
session opens, and it is somewhere to put an intermediate file. A one-shot run has one for as long as
it runs. A session that cannot be given one runs without one and says so.

`/status` reports it as the session's own, on a line of its own rather than among the rules, because
it is reachable without any rule covering it.

**A program a `run` starts is told where it is**, through `BRAVEBOT_SCRATCH_DIR` in the environment of
every stage of the line, in the background as in the foreground. An assignment of that name on the
line itself wins, as it would in a shell. A session with no directory of its own sets nothing, so a
program finds the name absent rather than pointing somewhere that is not there. The planner is told
the same path at the same point, since a variable nothing knows to read is a variable nothing reads.
`$TMPDIR` for such a program stays whatever it was.

The reason this exists is that the alternatives are worse. A file in the project is one a build, a
test run, a `git add -A` and a reviewer each have to deal with, and one somebody has to remember to
delete. Reaching `/tmp` instead would mean `/add-dir /tmp`, which vouches for whatever every other
process on the machine has left in a world-writable directory in the same action.

### What it is trusted for, which is nothing extra

A path under it resolves, so a file there is read, written, listed and searched by its absolute path.
The temporary directory it sits in is **not** reachable, and a path leaving the session's own
directory is refused exactly as one leaving the project is. Neither `/add-dir` nor `/cd` will take it.

Nothing is trusted for being there. The directory carries no rule of its own, so a path under it that
no rule covers is answered by whatever you said about the workspace, which trusts nothing where you
declined. Per file the answer is the one [a write records](#what-a-write-does), marking the exact path
written. A line whose output is untrusted distrusts the file it redirected into here as anywhere else,
which is what stops a turn reading its own untrusted output back as trusted.

It is a place to write, not a place where writes stop being asked about. A path under it appears in a
plan's write set, is shown in the prompt, and takes every write gate a path in the project takes. What
it buys is a fixed place that needs no name invented for it, that your repository does not report,
that no search has to skip, and that goes when the session does.

### Nothing in it outlives the session

The directory goes as the session ends, with everything in it, whether the session ended by being left
or by an error on the way out. A session that carries on from another is given its own: a resume, a
fork, and the session `/clear` begins each open a new one, and the directory belonging to the session
they followed is removed rather than handed on. A resume can come days later, and bytes surviving that
gap would be a cache nothing evicts.

An [undo](../using/sessions.md) does not reach in here. A file a turn wrote in this directory is
neither restored nor removed, and it is not named among the paths the rewind could not put back. What a
turn keeps so that it can be undone is a bounded amount shared by every file that turn wrote, and an
intermediate file is exactly the size of thing that bound is set to stay clear of: keeping one would
spend what a source file's own copy needed, so the file you want undone would lose to the file nobody
does.

## How long an answer lasts

**The map belongs to the session, not the directory.** Every session start asks, whatever any earlier
session in that directory answered, unless you pressed `r` there
([below](#remembering-the-answer)). `/clear` begins a session and therefore asks, on the same terms.

`--resume` does not ask: it restores the map from the record of the session you chose, because the
answer honoured is the one that session's own user gave. It also carries the rules that session's
writes recorded, which is what stops a resumed turn reading back a file an earlier turn of the same
session poisoned. A record from before maps were kept has none, and is asked about.

**What the record keeps is the name rather than the key.** A rule inside the project is written down
relative to it and a rule outside is written down in full, and a resume puts the resuming directory
back on the relative ones. That is what keeps a record about the same files after you move or rename
the checkout: full paths on disk would each name somewhere that is no longer there, nothing would
match, and the session would resume as though nobody had vouched for anything.

The question grants standing permission. Honouring last week's yes would grant it on behalf of a user
who was never asked, and trust assumed from silence is not trust granted. `r` is the one answer that
says it is meant to last, and it is honoured on narrower terms than a yes is given on.

### Remembering the answer

In the terminal interface the startup question offers a third key, `r`. It trusts the directory
exactly as `y` does and writes the answer down, one file per directory under `~/.bravebot/trusted`.
The question says what the key covers, how to take it back and the file it writes before you press
it.

A later session started in that directory is not asked, and says so as it opens:

```
trusting /home/me/projects/app (you said to remember it 3 days ago; /forget-trust to be asked again)
```

It covers less than a yes does:

- **Exactly that directory.** A session started in a directory inside it or above it is asked. A
  rule about a directory covers everything below it, so an answer kept about `~/projects` would
  otherwise answer for a session started in any repository cloned there afterwards. A session
  started in the remembered directory itself trusts everything below it, as a yes there does.
- **That directory, not its name.** The answer records which directory was at the path, by when it
  was made and, except on Windows, its number on the disk. One deleted and made again at the same
  path, such as a fresh clone, is asked about. On Windows the time is all there is to go on, and a
  clone deleted and replaced within seconds can be given the old one's time, so it is taken for it.
- **The rule a yes writes, and nothing else.** What the earlier session went on to record, a file it
  marked untrusted included, is not carried. That is the [first cost below](#known-costs), and a
  kept answer does not close it.
- **Not everywhere.** `r` is not offered at your home directory, a directory holding it or a
  filesystem root, where the filesystem cannot say when the directory was made, at a directory your
  settings file named, with `--dangerously-skip-permissions` (which asks nothing and writes nothing),
  or in an [incognito session](../using/sessions.md#a-session-that-leaves-nothing-behind). An
  incognito session still honours an answer an ordinary one kept.

Whenever it is in doubt it asks: a missing or unreadable record, one about an earlier directory at
the path, or a line about this directory that this build cannot read all mean the question is put.

`bravebot --plain` and the desktop app ask in every session for now and do not read the record.

[`/status`](#reading-the-map-back) says when a kept answer is in force and names its file.
[`/forget-trust`](../reference/commands.md#forget-trust) removes it: the session you type it in
keeps the map it has, and the next one started there asks.

## Reading the map back

```
/status
```

lists every rule in force, however many there are, so what a line vouched for does not have to be
remembered. The [session's own directory](#a-directory-of-the-sessions-own) gets a line of its own
there, since it holds no rule to list. Where a [kept answer](#remembering-the-answer) will trust the
working directory in later sessions, the line under it says so, when you gave it, and which file
holds it.

## `~/.bravebot` is not governed by this

Your own directory is read as trusted **by provenance** rather than by any rule here: putting a file
there is itself the grant. A project's own files are *not* covered by that and are read through the
map, whatever their names. See [Instructions](../customize/instructions.md#trust).

## Known costs

All of these are deliberate.

- **A fresh session forgets what an earlier one poisoned.** The rule that untrusted data marks its
  destination untrusted holds within a session and across a resume of it. Across a fresh start it
  cannot, because the map it was recorded in is gone. If a file holds content you do not trust, say no
  to the directory, or do not leave it there.

- **A file another process drops into a trusted directory is trusted.** A rule is about the path, so
  `npm install`, `git pull`, an editor, a background daemon, or a program the agent was allowed to run
  can all put a file inside a vouched-for tree and it will be read as trusted. This cannot be closed by
  watching the filesystem: by the time anything noticed, the question would be whether to distrust a
  file you may have created yourself, and asking that on every change would make the map useless.

  A [redirection](../reference/tools.md#a-redirection-is-a-write) is the one exception, because
  bravebot opens that file itself: what a line writes through `>` or `>>` is recorded, so a program's
  output redirected into a vouched-for tree marks the path it landed on untrusted. A file the program
  opens on its own, which is `cmd -o notes.txt` or anything a build writes, is not.

  Said plainly: **trusting a directory trusts what lands in it**, so a tree that a build or a
  dependency manager writes into is a tree you are vouching for ahead of time.

  The [session's own directory](#a-directory-of-the-sessions-own) meets this more often without
  changing what it is. Writing a file and reading it back is incidental in a project and is the whole
  purpose of that directory, and `cmd -o` into it is the ordinary case there rather than the odd one,
  so the redirection exception covers less of the traffic. No rule about that would help, since the
  same writes are available one directory up.

- **A kept answer trusts what arrived while nobody was asked.** A session started from `r` trusts its
  directory as a yes given at its start would, so both costs above apply to it. What it loses beyond a
  yes is the question itself: the moment after pulling a branch or adding a dependency when you might
  have answered `n`. The line the session opens with, `/status` and `/forget-trust` are what is left
  of it.

- **A symlink inside the project gives one file two names, and each name is its own rule.** A rule is
  keyed on the path an operation spelled, while confinement resolves where that path lands, and the
  two do not have to agree. So content written as untrusted through one name is read back as trusted
  under the other, which is the one way the round trip in [What a write does](#what-a-write-does)
  stays open. Two spellings of the same name are a single rule, so this is about a link in the tree
  and not about punctuation in a path. If a tree holds a link to somewhere else inside itself, that is
  a tree to say no to.

- **Confinement is settled before an operation runs, not while it runs.** Where a path lands is worked
  out by resolving it, and the read or the write happens after that. A tree already arranged to escape
  the project is refused. A tree rearranged in the window between the two, so that a directory
  resolved on the way in is a symlink by the time the file is opened, is not.

- **An undone turn's intermediate files stay until the session ends.** Nothing written in the
  session's own directory goes back, so a turn that wrote one there and was undone has left a file a
  later turn in the same session can read. The directory goes when the session does, and what is in it
  is the workings of a turn rather than anybody's work in progress.

- **Confinement does not keep a confined program out of the session's directory.** A program that
  cannot open a temporary file fails outright, so a [confined run](security.md) is allowed the system
  temporary directory, and the session's own directory is inside it. Narrowing that to the one
  directory would leave a compiler or a package manager nowhere to write. So a confined stage can read
  what a turn left there without its plan naming the path. A program `run` starts is unconfined, so
  this arrives with the profile rather than before it.

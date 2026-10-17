---
id: WORKTREE
title: A worktree of its own for a session
status: proposed
governs:
  - crates/agent/src/git.rs
  - crates/agent/src/git/checkout.rs
  - crates/agent/src/turn/prompts.rs
  - crates/agent/src/workspace.rs
  - crates/cli/src/continued.rs
  - crates/cli/src/main.rs
  - crates/cli/src/flags.rs
  - crates/session/src/sessions.rs
  - crates/tui/src/app.rs
  - crates/tui/src/checkouts_command.rs
  - crates/tui/src/resume.rs
  - crates/tui/src/status.rs
  - crates/ui-bridge/src/bridge.rs
documented-by:
  - none (gap: a page on starting a session in a worktree, resuming it and removing it, owed until the design is built)
---

## Scope

A session that works in a git worktree the driver made for it, so that its edits, builds and
programs run in a tree of their own and the person's working directory stays as it was: how a person
asks for one, what the driver makes, what the session may do in it, how a resume returns to it and
what becomes of it when the session ends.

Nothing here is built. The mechanism is [checkouts.md](checkouts.md), whose writer, location and
labelling a worktree reuses. A clause in this spec reads `verified-by: none` until the work it
describes lands.

A **worktree** here is a checkout, as checkouts.md defines one, that the driver makes for the session
itself and that is the session's working directory. Every other checkout is one a delegate was given.
The **project directory** is the working directory the session was started in, which holds the
repository.

What a checkout is, how it is written and what its files are labelled is
[checkouts.md](checkouts.md). What a record keeps and how a session is found again is
[sessions.md](sessions.md). The flags that start a session are in [cli.md](cli.md). The rule that
untrusted content never reaches the driver or the planner is [AGENTS.md](../../agents/AGENTS.md).

## What exists today

A person who wants a session on a second branch makes the worktree with git and starts bravebot in
it. Nothing in bravebot makes, names, resumes into or removes that worktree. A session started in one
works in a repository that [GIT-8](tools/read-git.md#GIT-8) declines, so `read_git` answers nothing
there and no delegate can be given a checkout of it.

A delegate can be given a checkout ([CHECKOUT-1](checkouts.md#CHECKOUT-1)). A session cannot.
[BG-14](background-sessions.md#BG-14) proposes that a background session work in one without being
asked.

## Clauses

<a id="WORKTREE-1"></a>
### WORKTREE-1: `--worktree` starts a fresh interactive session in a worktree made for it

`bravebot --worktree` and `bravebot --worktree=<name>` open the interactive session in the project
directory, wait for the answers the session opens with, make a worktree of the repository at HEAD, and
move the session into it before the first turn. The session says where the worktree is, and its name
when it has one, as it starts.

A name is given in the `=` form only. A word after the bare flag is not a name, so it is read as what
it would be without the flag: a subcommand or a task, which [WORKTREE-10](#WORKTREE-10) refuses.

The flag is refused where stdin or stdout is not a terminal, as `-i` is
([CLI-31](cli.md#CLI-31)): nobody would be there to see which tree the session works in.

From the first turn the worktree is the working directory: a relative path means a path in it, a
command starts in it, and the file tools reach it. The project directory is outside the session's
workspace, so the file tools do not reach it unless the person opens it as they open any directory
([CHECKOUT-7](checkouts.md#CHECKOUT-7)).

The worktree is made after the answers the session opens with, because a checkout is made only where
the map trusts all of the repository's `.git` ([CHECKOUT-4](checkouts.md#CHECKOUT-4)). A person who
declines the question gets no worktree, and the session ends before its first turn and says why.
Declining otherwise starts a session with nothing trusted ([TRUST-7](trust-map.md#TRUST-7)).

A refusal that needs the map, or the directories the opening questions open, waits for those answers.
Every other refusal is made before the terminal is taken, as [CLI-31](cli.md#CLI-31)'s are. A worktree
whose writing the person interrupts is removed with its entry, as a checkout refused while it is
written is ([CHECKOUT-5](checkouts.md#CHECKOUT-5)). A session killed outright leaves it, as it leaves a
checkout ([CHECKOUT-6](checkouts.md#CHECKOUT-6)).

The move is made before the first turn, with nothing yet typed, which makes it a smaller case than
moving a live session. The map, rules, sources and memory the session has from then on are the ones
[WORKTREE-4](#WORKTREE-4) and [WORKTREE-5](#WORKTREE-5) give it.

**Why a flag on the start.** The person decides before any file is written which tree the session
edits. A session that began in the project directory and moved later would have had a turn there first.

**Why the `=` form.** An optional argument that is a free word cannot be told from a subcommand or
from the first word of a task. `--worktree doctor` would start a session named `doctor`.

**Why the session does not start without it.** A session that fell back to the project directory
would edit the tree the person asked it to leave alone, which is what the flag is written to prevent
([CHECKOUT-2](checkouts.md#CHECKOUT-2)).

`verified-by: none`

<a id="WORKTREE-2"></a>
### WORKTREE-2: a name is a label of ASCII letters, digits, `.`, `_` and `-`, and decides no path

A name is ASCII letters, digits, `.`, `_` and `-`, begins with a letter or a digit, and is at most 64
characters. Any other name, and an empty one, is refused before anything is made, and the refusal says
what a name may hold. The flag without a name makes a worktree without one.

The name is kept in the record and shown wherever the worktree is named to the person: as the session
starts, in `/status`, in `/checkouts` and in the line printed as the session ends. A record is read as
a claim, so a stored name that does not meet this rule is read as no name, and the record is not
refused for it.

The name is not the worktree's directory and not its id. The worktree is a directory of its own in the
checkouts directory ([CHECKOUT-6](checkouts.md#CHECKOUT-6)), so a name decides nothing about a path, a
ref or an entry under `.git`. Two worktrees can carry one name.

**Why a label.** The sweep and the check a resume makes on where a checkout is take a checkout's
directory to be its number ([CHECKOUT-16](checkouts.md#CHECKOUT-16)), and removal names it by that
number ([CHECKOUT-15](checkouts.md#CHECKOUT-15)). A name in that place would put a person's typing
into a path the driver removes.

**Why these characters.** The name is printed into a line the person may paste into a shell and into
the transcript, so it holds nothing either would read as syntax, and nothing that looks like another
letter or reads in the other direction.

`verified-by: none`

<a id="WORKTREE-3"></a>
### WORKTREE-3: `--worktree` is refused where a delegate's checkout would be, and where a checkout would not be kept

The flag is refused, and the session goes no further, wherever a checkout is refused for a delegate:

- the project directory is not the root of a repository `read_git` opens, including a directory that
  is itself a linked worktree, or the repository holds a filter, an end-of-line conversion or a name
  git would not check out, or a tree past the bounds ([CHECKOUT-4](checkouts.md#CHECKOUT-4));
- the project directory holds the checkouts directory, as it does for a session in the home
  directory, or a directory the session opened at its start, from the person's settings, holds the
  project directory or the checkouts directory ([CHECKOUT-7](checkouts.md#CHECKOUT-7));
- the sandbox mode is `read-only`, which holds the file tools back from the entry a checkout writes
  under `.git` ([SANDBOX-22](sandboxing.md#SANDBOX-22)).

It is refused under `--incognito` as well, and on a machine with no state directory
([STATE-2](state-directory.md#STATE-2)). Both put a checkout in the system temporary directory, where
it goes with the session and with whatever was done in it
([CHECKOUT-6](checkouts.md#CHECKOUT-6)).

Each refusal names which of these it was, in the driver's words, and carries no text read from the
repository.

**Why a temporary worktree is refused.** A delegate's work in a temporary checkout is brought back
with `apply_checkout` before the session ends. A session's work has no such step, so a temporary
worktree would delete what the person did in it at the moment they leave.

`verified-by: none`

<a id="WORKTREE-4"></a>
### WORKTREE-4: a worktree is written, placed, labelled and ruled as a checkout is

The driver writes HEAD's tree and the entry git needs to know the worktree for, and starts no
program to do it: not git, and nothing a hook, `core.fsmonitor` or the configuration names
([CHECKOUT-5](checkouts.md#CHECKOUT-5)). It is made under the state directory, keyed by the project
directory ([CHECKOUT-6](checkouts.md#CHECKOUT-6)). Each file carries the label the same path has in
the project directory, and the rules the map holds under the project directory are copied to the
worktree before anything can reach it ([CHECKOUT-8](checkouts.md#CHECKOUT-8)).

Permission rules hold in the worktree as in the project directory
([CHECKOUT-9](checkouts.md#CHECKOUT-9)). A relative specifier is held against the worktree's root, so
a rule denying a read of `.env` refuses a read of `<worktree>/.env`. An absolute specifier under the
project directory is copied to the worktree as another spelling of the same rule, each time the rules
are read, which includes a resume.

A worktree's id is of a form no delegate's checkout has, so the sweep can tell the two apart without
opening either. It is one that no checkout and no worktree of the project holds, however many are
kept, so keeping many does not make the next one fail. A person names a worktree by it wherever a
checkout is named by its number.

The worktree is detached at HEAD. No branch is made and no ref under `refs/` changes. Changes the
person has not committed are not in it.

The trail records the worktree made as it records any checkout
([CHECKOUT-19](checkouts.md#CHECKOUT-19)).

**Why the same writer.** A second way of writing a tree would need its own case for each of the
refusals of [CHECKOUT-4](checkouts.md#CHECKOUT-4), and each exists because writing a file differently
from the stored blob, or into a name git refuses, puts bytes in the person's repository that nobody
vouched for.

**Why the copied rules.** A rule written for `/work/project/secrets/**` names a file by its full path.
In the worktree the same file has another full path, which no rule names unless the rule is copied.

**Why no branch.** A branch is a ref in the person's repository. The driver writes nothing there
beyond the worktree's entry. The person, or a line they approve, makes a branch with git.

`verified-by: none`

<a id="WORKTREE-5"></a>
### WORKTREE-5: the session works in the worktree as a delegate works in its checkout, and reads its sources from the project directory

The session's workspace is rooted at the worktree, as a delegate's is rooted at its checkout
([CHECKOUT-7](checkouts.md#CHECKOUT-7)). The directories the session opened at its start from the
person's settings, and any it opens later, stay reachable as they were.

The sources are the project directory's `AGENTS.md`, skills, definitions and `.bravebot/settings.json`,
composed each turn as for any session ([INSTR-7](instructions.md#INSTR-7)). A directory the session
has worked in adds the `AGENTS.md` and `.bravebot/skills` at the same relative path in the project
directory ([INSTR-14](instructions.md#INSTR-14)). Nothing in the worktree is read as a source
([CHECKOUT-9](checkouts.md#CHECKOUT-9)).

The planner is told, in the driver's words, the commit the worktree holds, that changes the person
has not committed are not in it, which paths a deny rule left out, that a `git fetch` in it updates
the refs of the project directory, and that it shares one stash with the project directory, so it is
not to use `git stash` ([CHECKOUT-7](checkouts.md#CHECKOUT-7)). The person is told the same as the
session starts.

What follows for a delegate follows for the session:

- It keeps no definition memory ([CHECKOUT-9](checkouts.md#CHECKOUT-9)).
- A command vouched for in the project directory is asked about again
  ([CHECKOUT-10](checkouts.md#CHECKOUT-10)).
- It is offered no `lsp` ([CHECKOUT-20](checkouts.md#CHECKOUT-20)).
- A delegate it starts shares the worktree, and a spawn asking for a checkout is refused
  ([CHECKOUT-3](checkouts.md#CHECKOUT-3)).
- `read_git` answers from the entry the driver wrote, never from `<worktree>/.git`
  ([CHECKOUT-12](checkouts.md#CHECKOUT-12)).

**Why the sources stay.** A worktree holds HEAD's copy of the project's configuration. A session that
read it would have a second copy of the project's rules, which the session could change by writing
there.

**Why the planner is told.** It reads HEAD's files with nothing else to say they are not the file the
person was just editing, and a stash is one stack for every worktree of the repository.

`verified-by: none`

<a id="WORKTREE-6"></a>
### WORKTREE-6: the record is kept under the project directory and names the worktree

The record is written when a turn ends, as any record is, and under the project directory, not under
the worktree. `bravebot --resume`, `--continue` and the picker, run in the project directory, find it,
which is the directory the person is in when they type the command printed as the session ends.

The record lists the worktree with the other checkouts the session keeps
([CHECKOUT-15](checkouts.md#CHECKOUT-15)), says that the session works in it, and holds its name. A
record written before worktrees existed reads as having none.

Rules the map holds under the worktree are written in full, as a rule outside the project is
([TRUST-6](trust-map.md#TRUST-6), [CHECKOUT-8](checkouts.md#CHECKOUT-8)), and never relative to the
project directory. A resume reads each as a rule about the worktree.

**Why under the project directory.** The list a person reads when resuming is the list for the project
([SESSION-1](sessions.md#SESSION-1)). A record under the worktree's key would be found only by a
person who knew the worktree's path.

**Why in full.** A rule recorded relative to the project directory is read back under whichever
directory the resume starts in. A rule vouching for `src/a.rs` in the worktree would become one vouching
for the person's own `src/a.rs`, a file nobody vouched for.

`verified-by: none`

<a id="WORKTREE-7"></a>
### WORKTREE-7: a resume returns to the worktree, and is refused where it cannot

A resume that finds a worktree in the record moves the session into it before the first turn. The
record is read as a claim, on the terms of [CHECKOUT-16](checkouts.md#CHECKOUT-16): the worktree
returns only where its directory is the one the session would have made, is a directory and not a
link, has its entry under this repository's `.git`, and its commit is a full object id. The map comes
back from the record as it does for any resume ([TRUST-6](trust-map.md#TRUST-6)).

Where the worktree does not qualify, or a running session holds its lock
([CHECKOUT-16](checkouts.md#CHECKOUT-16)), the resume is refused before the first turn. The lock is
taken on Unix only, so on Windows a resume is refused on the other ground alone. The refusal
names the worktree and says why, and where the worktree is gone it says that `bravebot --fork <id>`
carries the conversation into the project directory.

Only a start with `--resume`, `--continue` or the picker, and the process `/bg` starts
([WORKTREE-9](#WORKTREE-9)), returns to a worktree. `/resume` of such a record from a running session
is refused, and says to start `bravebot --resume <id>`. The desktop
application refuses to open a record that names a worktree.

A one-shot run that continues a record with a task ([CLI-25](cli.md#CLI-25)) works in the directory it
was started in, so it cannot enter a worktree. `--continue` or `--resume <id>` with a task is refused
where the record names one, and the refusal names the worktree and says to resume it without a task.
`--continue` takes the newest record, as [SESSION-14](sessions.md#SESSION-14) says, and never passes
over one that names a worktree.

`--fork` of such a record starts in the project directory, says so, and carries no worktree
([SESSION-18](sessions.md#SESSION-18)).

The line printed as the session ends ([SESSION-8](sessions.md#SESSION-8)) names no directory for the
move into the worktree, because the command it prints is run in the project directory.

**Why refuse.** A resume that continued in the project directory would make the edits the conversation
was about to the person's own tree, under the same mode and the same approvals, with a note as the only
sign. That is the outcome [WORKTREE-1](#WORKTREE-1) refuses at the start.

**Why `--fork` is the way out.** It is the person's choice to leave the worktree, made with the
conversation's id, and the copy it makes is a new session that says where it works.

`verified-by: none`

<a id="WORKTREE-8"></a>
### WORKTREE-8: a worktree is kept until the person removes it, and the sweep never takes one

A worktree is kept when the session ends, whatever was done in it. The line printed as the session
ends ([SESSION-8](sessions.md#SESSION-8)) names it with its name and its path, says that it is a git
worktree of the project which the person's own git lists and removes, and says that a commit made in
it is on no branch and is lost to a garbage collection once the worktree is removed, unless the person
makes a branch first. It gives the command that resumes the session where a record was written. It
names the worktree where none was, which [SESSION-8](sessions.md#SESSION-8) would otherwise leave
unsaid.

The opening sweep never takes a worktree ([CHECKOUT-16](checkouts.md#CHECKOUT-16)), whether or not a
record lists it, and tells one from a leftover checkout by its id ([WORKTREE-4](#WORKTREE-4)). Deleting a session's record ([SESSION-30](sessions.md#SESSION-30)) does not delete
the worktree it named.

**Why kept whatever was done.** The person is given the path and can edit files or commit there with
nothing started by bravebot. The driver's record of its own file effects and programs does not show
that, and deciding from a comparison of the files with HEAD is a decision on bytes nobody vouched for
([CHECKOUT-15](checkouts.md#CHECKOUT-15)). A worktree removed because the record showed nothing would
remove their work.

**Why the sweep skips it.** A worktree is the session's whole working tree. Whether a record lists
it is a fact about the store, and a record can be deleted, lost, or rewritten by a front end that does
not list checkouts.

`verified-by: none`

<a id="WORKTREE-9"></a>
### WORKTREE-9: a command that begins another session works where this says, and `/cd`, `/branch` and an apply to the worktree itself are refused

`/clear` changes the conversation and not the directory, so the session it begins works in the same
worktree. `/handoff` begins its session as `/clear` does ([SESSION-34](sessions.md#SESSION-34)) and
works there too. Both ask the opening questions again, as a start in the project directory does
([TRUST-6](trust-map.md#TRUST-6)), and copy the rules the map then holds under the project directory
to the worktree before anything can reach it ([WORKTREE-4](#WORKTREE-4)). A question declined starts
the session with nothing trusted ([TRUST-7](trust-map.md#TRUST-7)) and does not end it, as it ends a
start ([WORKTREE-1](#WORKTREE-1)), because the worktree already exists. The record the new session
writes is kept under the project directory and lists the worktree ([WORKTREE-6](#WORKTREE-6)). The
record of the session left lists it too and stays resumable. The two never run at once, because the
process holds the worktree's lock through the change ([CHECKOUT-16](checkouts.md#CHECKOUT-16)).

`/resume` ends the session ([CMD-14](commands.md#CMD-14)), and the next works where the record chosen
says, as a start with `--resume` of it would. A record that names no worktree works in the project
directory, and the session says so as it starts. A record that names a worktree is refused
([WORKTREE-7](#WORKTREE-7)). The worktree left is kept ([WORKTREE-8](#WORKTREE-8)), and the lock on it
goes with the session.

`/bg` hands the record to a process that resumes it ([BG-2](background-sessions.md#BG-2)), and that
resume returns to the worktree ([WORKTREE-7](#WORKTREE-7)). The foreground session lets go of the
worktree's lock after it writes the record and before it starts the process, so the lock is no ground
to refuse the resume. [BG-14](background-sessions.md#BG-14) makes no checkout for that session, because
it already works in one.

`/init` writes in the working directory ([CMD-13](commands.md#CMD-13)), which is the worktree, where
[WORKTREE-5](#WORKTREE-5) has nothing read as a source.

`/cd` is refused ([CHECKOUT-16](checkouts.md#CHECKOUT-16)), and so is `/branch`
([SESSION-31](sessions.md#SESSION-31)), because the record lists the worktree with the checkouts the
session keeps ([WORKTREE-6](#WORKTREE-6)). Each refusal names the worktree and says that the person
leaves the session and starts another, since `/checkouts remove` cannot remove the worktree the session
works in. `apply_checkout` and `/checkouts apply` are refused for the worktree the session works in,
since the working directory they write into is that worktree.

**Why.** `/clear` and `/handoff` change the conversation and leave the directory. A person who asked
for a worktree asked to work out of it, so a session that left it would edit the tree they asked to
leave alone ([WORKTREE-1](#WORKTREE-1)). `/resume` and `/bg` begin from a record, and a record says
where its session worked, so the directory is the record's. `/init` writes where every write goes.
`/cd` moves the root out from under the worktree, and `/branch` carries on in a record that does not
list it, which are the reasons [CHECKOUT-16](checkouts.md#CHECKOUT-16) and
[SESSION-31](sessions.md#SESSION-31) refuse them for any checkout. An apply writes to the working
directory, which is the worktree itself.

`verified-by: none`

<a id="WORKTREE-10"></a>
### WORKTREE-10: `--worktree` composes with no other way of starting a session

`--worktree` composes with the flags that apply to every way of starting a session, which are taken
out of the line before the way of starting is chosen, such as `--settings`, `--agent`, `--safe`,
`--locked`, `--vet` and `--dangerously-skip-permissions`, except `--incognito` and `--sandbox read-only`,
which [WORKTREE-3](#WORKTREE-3) refuses. It is refused, naming the other word,
wherever it stands on the line, with `--resume` and
`-r`, `--continue` and `-c`, `--fork` and `-f`, `--from-pr`, `--bg`, `--plain`, `-i` and
`--interactive`, `-p` and `--print`, any option that belongs to a one-shot run (`--model`,
`--effort`, `--mode`, `--file`, `--add-dir`, `--json` and the rest of that list), any subcommand, and
a task given as words. A session started by `--resume` returns to the worktree its record names
([WORKTREE-7](#WORKTREE-7)) and is not given another.

**Why.** Each of those starts a session by a path that has its own idea of which directory it is in:
a record, a process the supervisor owns, a session in lines, a one-shot run. Giving each a worktree is
a decision about that path, and none is made here.

`verified-by: none`

## What this changes in other specs

None of the following is true of a build that has no session worktrees. Each is an edit to that spec
made in the change that builds this one.

- **[checkouts.md](checkouts.md).** The definition of a checkout in its Scope says a delegate is given
  it. A worktree is a checkout the session is given. [CHECKOUT-7](checkouts.md#CHECKOUT-7) says a
  checkout is reachable from no run but its delegate and the delegates that one starts, and gains the
  session. [CHECKOUT-14](checkouts.md#CHECKOUT-14) writes to the working directory and refuses the
  worktree the session works in. [CHECKOUT-15](checkouts.md#CHECKOUT-15) ends a checkout with its
  delegate, and a worktree is kept past the session. [CHECKOUT-16](checkouts.md#CHECKOUT-16) keys a
  checkout under the working directory and its sweep and restore read that key. For a worktree they
  read the project directory's, and the sweep takes none. Its `/cd` refusal says to remove the checkout
  with `/checkouts remove`, which cannot remove the worktree the session works in, so the refusal for a
  worktree says something else. [CHECKOUT-21](checkouts.md#CHECKOUT-21) lists a checkout with the number of the delegate given it,
  which a worktree does not have, and says the session `/clear` begins lists none of the checkouts
  made before it, which does not hold for the worktree. [CHECKOUT-16](checkouts.md#CHECKOUT-16) says
  a fork leaves no two records listing one directory, and a `/clear` or `/handoff` in a worktree
  session leaves two, one after the other.
- **[sessions.md](sessions.md).** [SESSION-1](sessions.md#SESSION-1) keys a session by the directory
  it ran in, which for a session in a worktree is the project directory.
  [SESSION-3](sessions.md#SESSION-3) gains the field that says which checkout the session works in.
  [SESSION-8](sessions.md#SESSION-8) names a kept worktree, which it says nothing yet builds, names
  one where no record was written, and prints no directory for the move into one.
  [SESSION-13](sessions.md#SESSION-13) and [SESSION-30](sessions.md#SESSION-30) leave a worktree where
  it is. [SESSION-31](sessions.md#SESSION-31)
  refuses `/branch` while the session keeps a checkout and tells the person to remove it with
  `/checkouts remove`, which cannot remove the worktree the session works in, so the refusal for a
  worktree says something else.
- **[tools/read-git.md](tools/read-git.md).** [GIT-8](tools/read-git.md#GIT-8) opens a linked worktree
  only at the root of a checkout a delegate was started in, and gains the session's.
- **[commands.md](commands.md).** [CMD-14](commands.md#CMD-14) has `/resume` start the next session in
  the same process as `--resume` does, and refuses a record that names a worktree.
  [CMD-13](commands.md#CMD-13) says a file `/init` writes is read from the next turn on, which does not
  hold for one written in a worktree.
- **[incognito.md](incognito.md).** `--incognito` is refused with `--worktree`.
- **[sandboxing.md](sandboxing.md).** [SANDBOX-18](sandboxing.md#SANDBOX-18) refuses a confined stage
  whose session directory is inside the state directory, which is where the worktree is. Which way
  that is resolved is open, under Not decided.
- **[instructions.md](instructions.md).** [INSTR-14](instructions.md#INSTR-14) reads a directory the
  session has worked in below the project root, and reads it in the project directory for a worktree.
- **[trust-map.md](trust-map.md).** [TRUST-7](trust-map.md#TRUST-7) starts a session whose startup
  question was declined, and a declined question here ends the session before its first turn.
  [TRUST-6](trust-map.md#TRUST-6) records a rule inside the project relative to it, and a worktree's
  rules are recorded in full. It has `/clear` ask the startup question again, and a worktree session
  that asks copies the answer's rules to the worktree again.
- **[state-directory.md](state-directory.md).** The list of what `checkouts/` holds gains the
  worktrees, and says a session's worktree is kept past its session.
- **[cli.md](cli.md).** A clause for the flag, and the usage text. [CLI-25](cli.md#CLI-25) refuses a
  task given with a record that names a worktree.
- **[background-sessions.md](background-sessions.md).** [BG-14](background-sessions.md#BG-14) gives a
  background session a checkout of its own. The mechanism is the one here, and the two do not share a
  flag. A background session falls back to the working directory outside a repository, and no one
  asked for its checkout. [WORKTREE-3](#WORKTREE-3) refuses instead, because the person asked for this
  one. A background session's checkout takes retention, record placement and the sweep from this
  spec. [BG-2](background-sessions.md#BG-2) starts the process `/bg` makes in the working directory
  the foreground ran in. For a worktree session that process starts in the project directory, where the
  record is kept, and returns to the worktree ([WORKTREE-9](#WORKTREE-9)).

## Not decided

- **How a program the session runs is confined.** The sandbox refuses a confined stage whose session
  directory lies inside the state directory, because `~/.bravebot` is a credential location
  ([SANDBOX-18](sandboxing.md#SANDBOX-18)), and [WORKTREE-4](#WORKTREE-4) makes the worktree there. As
  specified, a confined `run` in a worktree session is refused. Three answers exist:
  a worktree is a session directory the sandbox admits while the rest of `~/.bravebot` stays refused,
  the worktree is made somewhere else, or the session's stages are rooted at the project directory.
  Rooted at the worktree the stage is refused. Rooted at the project directory it can write the
  person's tree and not the worktree, the opposite of the Scope. Each answer changes a clause in
  sandboxing.md, and the second changes [WORKTREE-4](#WORKTREE-4)'s placement, so none is taken here
  and the design cannot be built until one is.
- **`/worktree [name]`.** Moving a live session into a worktree changes the root under a trust map, the
  permission anchors, the language servers, the hooks, the definition memory and the lines already
  queued. `/cd` re-spells the map for a new root ([TRUST-13](trust-map.md#TRUST-13)) and does none of
  the copying a checkout needs.
- **Asking at the end, with counts.** A line that asks keep or remove would show the files changed and
  the commits in no other branch. No read computes the commits, and a program in the worktree can write
  the refs that answer it, so the counts could be shown to the person and decide nothing
  ([CHECKOUT-15](checkouts.md#CHECKOUT-15)). [WORKTREE-8](#WORKTREE-8) keeps every worktree and asks
  nothing.
- **Removing a kept worktree from bravebot.** `/checkouts` lists the checkouts the session keeps, and a
  session started in the project directory keeps none. Listing the project's worktrees from its
  records, and removing one no running session holds, would answer it. Until then the person's own
  git removes one.
- **Rewind.** A rewind puts back nothing in a checkout and records the gap
  ([CHECKOUT-17](checkouts.md#CHECKOUT-17)). A session working wholly in a worktree would have no
  `/undo` of its own edits unless the worktree used the session's backups in place of a delegate's.
- **A second session in a kept worktree.** Naming one on the command line would put two records over
  one directory at once, which [CHECKOUT-16](checkouts.md#CHECKOUT-16) keeps from happening. A
  `/clear` or `/handoff` leaves two, one after the other ([WORKTREE-9](#WORKTREE-9)).
- **`/resume` of a record that names a worktree.** [WORKTREE-7](#WORKTREE-7) refuses it from a running
  session, because allowing it moves a running process into a worktree, as `/worktree` would.
- **Where the answers that outlast a session are kept.** Remembered command lines, the permission
  grants a person gave, the answers about credentials, a kept startup yes and the reach a person
  remembered are each keyed by the working directory. Keyed by the worktree they are orphaned under a
  path used once. Keyed by the project directory they carry into a tree
  [CHECKOUT-10](checkouts.md#CHECKOUT-10) asks about again.
- **Whether a name earns its place.** It is a label, and nothing finds a worktree by it. Re-entering a
  worktree by name would give it a use, and needs the answer to the second-session question above.
- **A base other than HEAD,** a pull request in place of a name, a setting for where worktrees go, and
  directories linked in from the project, such as a dependency cache.
- **What the terminal title and the info panel show** as the directory.
- **What the desktop application shows of a worktree.** [checkouts.md](checkouts.md) has the window
  list, show and apply none of a delegate's checkouts, and this says no more of a worktree. It refuses
  a record that names one ([WORKTREE-7](#WORKTREE-7)).

## Known costs

- **The worktree is HEAD alone.** A change the person has not committed is not in it, and neither is a
  build directory or a dependency cache, so the first build in it is a cold one.
- **No branch.** Work in a worktree is on no branch until the person, or a git line they approve,
  makes one.
- **A repository with a filter, an end-of-line conversion or LFS gets no worktree**
  ([CHECKOUT-4](checkouts.md#CHECKOUT-4)).
- **The session loses what a checkout loses.** No definition memory, no `lsp`, a first build that is
  asked about again, and no delegate in a checkout of its own.
- **`/init` writes where nothing reads it as a source.** The file is in the worktree until the person
  brings it into the project directory with git.
- **`/cd` and `/branch` are refused.** The person leaves the session and starts another
  ([WORKTREE-9](#WORKTREE-9)), as for any kept checkout.
- **The refs and the stash are the project's.** A `git fetch` in the worktree updates the project
  directory's remote-tracking refs, and one `git stash` stack is shared
  ([CHECKOUT-5](checkouts.md#CHECKOUT-5)).
- **A worktree is not a sandbox.** Where a program does run in the worktree, it is as unconfined as any
  other and can write the project directory by an absolute path ([sandboxing.md](sandboxing.md)).
- **Every worktree stays.** Each holds a copy of the tree on disk until the person removes it.
- **A definition that asks for a checkout cannot be spawned.** A spawn asking for a checkout is refused
  in a checkout ([CHECKOUT-3](checkouts.md#CHECKOUT-3)), and a definition asking for one is refused
  where a call asking would be ([CHECKOUT-2](checkouts.md#CHECKOUT-2)), though its delegate would share
  a worktree that is already separate.
- **A worktree no record lists is found by the path the session printed, or by git.** A deleted record
  leaves it where it is, and a session killed outright prints nothing.
- **A resume whose worktree is gone is refused.** The conversation is carried on only by a fork, which
  works in the project directory.
- **A build older than this drops the record's worktree field** when it saves the record, and a later
  resume then reads a record with no worktree and works in the project directory.
- **On Windows nothing stops two sessions in one worktree.** The lock that refuses a second is taken on
  Unix only ([CHECKOUT-16](checkouts.md#CHECKOUT-16)).
- **A name is not unique.** Two worktrees can carry one, and a name finds nothing.
- **The directory is under the state directory.** An editor or tool opened on the project's path does
  not show the worktree, and the person opens the path the session printed.

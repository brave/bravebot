---
id: COMMIT
title: A commit for each turn, and its undo
status: proposed
governs:
  - crates/config/src/settings.rs
  - crates/agent/src/workspace.rs
  - crates/agent/src/rewind.rs
  - crates/session/src/rewind.rs
  - crates/session/src/sessions.rs
documented-by:
  - none (gap: a page on the `commit` setting and on taking a turn's commit back, owed until the design is built)
---

## Scope

A setting that has the driver commit the files a turn wrote, in the repository the session works in,
and an undo that takes that commit back with the turn: when a commit is made, what it holds, who
writes its message, how git is started, what is said and recorded about it, and what a rewind does to
it.

Nothing here is built. A clause in this spec reads `verified-by: none` until the work it describes
lands.

A turn's **paths** are the paths the file tools recorded as written while the turn ran, which is the
record a rewind puts back ([SESSION-19](sessions.md#SESSION-19)). A path a program started by `run`
wrote is not among them, and neither is one a delegate wrote in a checkout of its own
([CHECKOUT-17](checkouts.md#CHECKOUT-17)). A file the planner brings back from a checkout with
`apply_checkout` is a write in the working directory ([CHECKOUT-14](checkouts.md#CHECKOUT-14)), so it
is one of the turn's paths. One a person brings back by typing `/checkouts apply` is not a turn's.

The **repository** is the one `read_git` opens for the session's working directory
([GIT-2](tools/read-git.md#GIT-2), [GIT-8](tools/read-git.md#GIT-8)). A session whose working
directory is in no repository has none, and nothing here applies to it or says anything to it.

What a write records in the trust map is [trust-map.md](trust-map.md). What `attribution` carries is
[BACKEND-30](backends.md#BACKEND-30). The rule that untrusted content never reaches the driver or the
planner is [AGENTS.md](../../agents/AGENTS.md).

## What exists today

A commit exists only where the planner asks `run` for `git commit`, a person approves the line, and
the stage's confinement lets git write the repository's `.git`
([SANDBOX-14](sandboxing.md#SANDBOX-14), [SANDBOX-18](sandboxing.md#SANDBOX-18)). The `attribution`
setting says what such a message may carry ([BACKEND-30](backends.md#BACKEND-30)).

`/undo` and `/rewind` put files back from copies the session keeps, and neither
[SESSION-19](sessions.md#SESSION-19) nor [SESSION-21](sessions.md#SESSION-21) says anything of a branch
or a commit. A turn therefore leaves no commit a person can bisect or revert, and a commit the planner
made stays after its turn is rewound.

## Clauses

<a id="COMMIT-1"></a>
### COMMIT-1: `commit` is a setting with two values, `off` and `turn`, and only the person's own files set it

`commit` holds `off` or `turn`. A name no file wrote is `off`. A value that is not a string, or a
string that is neither word, reads as `off` and is reported, so a misspelling never turns commits on.

It is read from the home layer and from the file `--settings` names. A project or local layer that
names it is ignored and reported, as one that names `provider` or `attribution` is
([backends.md](backends.md)). A file `--settings` names that resolves inside the workspace is a
checkout's file under another name and is ignored as well.

**Why `off` by default.** A commit is written into a history other people read and push. A person who
has not asked for one has said nothing about how that history should look.

**Why only the person's files.** The setting has the driver write under `.git` in every repository the
session works in. A checkout arrives with whatever produced it and nobody vouched for what it says, so
a setting in one must not have the driver write history. The keys that name where the driver acts are
read from the person's files alone on the same footing.

`verified-by: none`

<a id="COMMIT-2"></a>
### COMMIT-2: a turn that ends with a reply makes one commit, in a fixed order, and a turn that does not end with one makes none

With `turn`, a turn that ends with a reply and wrote at least one path in the repository makes one
commit of the paths [COMMIT-3](#COMMIT-3) chooses. It is made after the delegates the turn started have
ended, so none of them is still writing, and before the `turn-finished` hook fires
([HOOK-2](hooks.md#HOOK-2)). A background job the turn left running is not waited for. A turn that is
stopped or fails commits nothing: its files stay in the working tree, uncommitted, for the person to
look at.

The order is fixed. The status is read and the paths are chosen, the credential scan included. The
message is requested ([COMMIT-5](#COMMIT-5)). The paths are staged and committed. A stop before the
staging starts commits nothing and starts no git. After it starts, the staging and the commit are
carried through together, to the bound [COMMIT-6](#COMMIT-6) sets.

Nothing is committed where HEAD is detached or names a branch with no commit yet. Where git is
partway through a merge, a cherry-pick or a revert it refuses a commit of named paths, and nothing is
committed then either.

The commit goes on the branch HEAD names, and nothing is pushed. No other branch, no tag and no remote
is made or changed.

**Why a stopped turn commits nothing.** The files a stopped turn left are partly done. A message for
them would describe a result the turn did not reach, and `/undo` still takes them back.

`verified-by: none`

<a id="COMMIT-3"></a>
### COMMIT-3: the paths committed are the turn's own that three conditions leave in, and nothing else

A path of the turn is committed only where all three hold. A path that fails one is left out and,
where [COMMIT-8](#COMMIT-8) gives it a cause, named with it.

1. The path the write resolved to, links followed, lies in the repository's working tree, and not
   under a `.git` and not inside another repository or a submodule.
2. `read_git`'s status ([GIT-10](tools/read-git.md#GIT-10)) lists it as changed, created or removed
   when the turn ends, and does not list it as not compared. The status is read with each untracked
   file listed and not its directory, whatever `status.showUntrackedFiles` says, and a name is matched
   to the listing as the trust map matches names ([TRUST-2](trust-map.md#TRUST-2)). A path the status
   does not list holds what HEAD holds, is ignored, or is one a deny rule covers, and adds nothing. A
   path it does not compare is one a filter, an attribute or a line-ending conversion would change on
   its way into git, and it is never staged. A path listed as created is committed only where the
   turn's own record says the file did not exist before the turn. A turn whose paths all go unlisted
   makes no commit.
3. The credential scan finds no value in what the commit would add for it, whoever put the value
   there and whatever answer a write of it was given ([CRED-16](credential-protection.md#CRED-16)).
   A commit is a place a turn must not copy a credential to ([CRED-11](credential-protection.md#CRED-11)).

The status is answered only where the map vouches for the whole working tree
([GIT-11](tools/read-git.md#GIT-11)), which covers every path in it. A turn that wrote untrusted data to
a path ([TRUST-4](trust-map.md#TRUST-4)) leaves the status unanswerable, and the turn commits nothing.

A path that held changes before the turn is left out as [COMMIT-4](#COMMIT-4) says.

A commit names exactly the chosen paths. It never names a directory, `.` or a pattern and is never made
with `-a`, `-A` or `--all`, so a file the person changed that the turn did not write is not in it, and
neither is what the person had staged in another path. A path's own staging is replaced by the commit's.

**Why exactly the recorded paths.** A commit that took the whole tree would carry the person's own
unfinished work into a message that describes the turn, and could not be taken back without taking
that work with it. The recorded paths are the driver's own account of what the turn did, which is why
they are the ones it may name.

**Why these three.** The first keeps the commit to a tree git would commit anyway. The second is a
question git answers from files in the repository, and the reader answers it without starting a program
and says where it cannot tell. A file that existed before the turn and was ignored is not one the turn
may add by editing the ignore rule. The third is the one effect of a commit a later edit does not undo:
an object stays in history after the file is changed.

`verified-by: none`

<a id="COMMIT-4"></a>
### COMMIT-4: a path that held changes before the turn is left out of its commit, so the commit holds only the turn's edits

Before the turn's first write to a path in the repository, and once for the turn, the status is read
as [COMMIT-3](#COMMIT-3) reads it. Each path it lists held changes before the turn: edited, created,
removed, staged, untracked or in conflict. The turn's commit leaves out every such path the turn
writes, with the cause `held earlier changes`, and holds only paths that were clean when the turn
began. The turn still writes them.

Where the status cannot be answered before the first write, or at the end, the turn commits nothing and
the note says `status not available`.

No git is started for the read, since the reader starts no program, and a turn that writes no path in
the repository reads nothing.

**Why.** Without it a turn that edits a file the person was already changing commits both together,
under a message that describes only one of them, and an undo cannot separate them. Because only clean
paths are committed, a rewind of the turn returns each path to exactly what its parent holds.

**Why once, before the first write.** The status hashes every changed file and reads every ignore and
attributes file. Read before each write it would cost a turn that edits two hundred files two hundred
reads, and a read after a write cannot tell what the turn changed from what was already there.

`verified-by: none`

<a id="COMMIT-5"></a>
### COMMIT-5: the message is one line the planner writes, a fixed one when it cannot be used, and `attribution` follows it

Where the turn's commit has a path to commit, the planner is asked for its message once, in a request
over a copy of the conversation, with a fixed question written in this program, offering no tools and
changing nothing in the exchange, as `/recap` asks its question ([CMD-16](commands.md#CMD-16)). The
request goes to the model `summaryModel` names, and to the session's own where it names none, as a
goal check does ([GOAL-19](goal.md#GOAL-19)). The question asks for one line of at most 72
characters saying what the turn changed. No request is made for a turn with nothing to commit, and none
where the exchange has met something untrusted, since the gate would not return its answer visible:
the fixed message is used. Where the model `summaryModel` names is one
[COMPACT-15](compaction.md#COMPACT-15) refuses or asks before sending to, no request is made and
nobody is asked, and the fixed message is used.

The answer is used only where the gate that decides what the planner may hold returns it visible, as an
aside's answer is written down only there ([SESSION-20](sessions.md#SESSION-20)). Of what is used,
the first line with anything on it is taken, control characters and the characters that draw as nothing
([PROMPT-13](prompting.md#PROMPT-13)) are removed, and the line is cut at 72 characters on a character
boundary. The line is scanned as a write is ([CRED-16](credential-protection.md#CRED-16)). A line that
is empty after that or holds a value the scan finds, an answer the gate did not return visible, and a
request that fails all give the fixed message `Changes made by a bravebot turn`. The driver adds
nothing to the line that it read from a file, a tool result or the repository.

A non-empty string the `attribution` block names for `commit` follows the line after one blank line,
word for word ([BACKEND-30](backends.md#BACKEND-30)). A block that names none, or an empty string,
adds nothing.

The message is passed to git as one argument and is read by no shell.

**Why the planner.** The planner holds the conversation and the person's request, so it can say what
the turn was for where the driver could only count files.

**Why the gate decides, and a fixed message otherwise.** A conversation that has met something
untrusted gives answers the driver may not act on. Choosing between the planner's line and the fixed
one reads the gate's label and never the line, which is the decision the rule allows. The fixed
message keeps a message that cannot be used, a cosmetic fault, from costing the turn its commit.

**Why the sentence is not translated.** The message is written into a repository whose other readers
the person's language setting says nothing about. The fixed sentence stays as it is, and the planner's
line is in the language it answers in.

`verified-by: none`

<a id="COMMIT-6"></a>
### COMMIT-6: the driver starts git with arguments it fixed, and takes from it an exit status and the branch it left

The driver starts git itself, as a program it found the way every program it starts is found, never
one a confined stage could have written ([SANDBOX-30](sandboxing.md#SANDBOX-30)). It is given the
reduced `PATH`, so a program git looks up is not one a confined stage could have written, and
inherits only the names git needs to find the person's own configuration and identity, never a `GIT_`
variable and never one that names a program. The arguments are fixed by this program, apart from the
message, which is one argument, and a ref name and ids the driver read from a repository the map
vouches for ([COMMIT-9](#COMMIT-9)). The paths are given on standard input, NUL separated and read
literally, and never as arguments. No shell is involved.

The subcommands started are `add` and `commit` to commit the paths, and `reset` and `update-ref` to
unstage them or to take a commit back ([COMMIT-9](#COMMIT-9)). `status` is not one of them: the read is
the reader's. `push`, `fetch`, `pull`, `remote`, `config`, `branch` and `tag` are never started.

Each start is bounded at 30 seconds, as a hook is ([HOOK-7](hooks.md#HOOK-7)), and one not over by
then is ended. A staging or a commit that git refuses commits nothing, and a commit refused after the
paths were staged is followed by an unstaging of them. The note says git refused, with its exit status
([COMMIT-8](#COMMIT-8)). An index another program holds locked is such a refusal.

Whether a commit was made is learned by reading the branch back after git ends, as the reader reads it,
and not from what git printed. The commit is the driver's where the branch names a commit whose parent
was the tip before the start and whose tree differs from that parent's in at least one chosen path and
in no other, whatever status git ended with. A chosen path the commit does not hold is named
([COMMIT-8](#COMMIT-8)). The exit status is kept for the note, and what git printed decides
nothing. The driver reads nothing under `.git` that the map has not vouched for. The map records
nothing for what git writes there, as it records nothing for what a program `run` starts writes
([SESSION-19](sessions.md#SESSION-19)), and the directory stays vouched as it was
([TRUST-2](trust-map.md#TRUST-2)).

Making a commit asks nobody. The setting is the person's own endorsement of one effect, whose
destination is the repository the session works in and whose content is paths and a message the driver
chose, so no tool is added and no routing field is left to the planner. A `deny` rule is about a line
`run` is asked for ([permissions.md](permissions.md)) and does not stop this commit.

**Why standard input.** A turn can write more paths than a command line holds, and a path read from
standard input is never taken for an option or a pathspec.

**Why the branch is read back.** A start ended at its bound may already have moved the ref, and an exit
status says nothing of that. A commit another program made while the driver's start ran is told from
the driver's by its tree.

**Why the environment is narrowed.** `GIT_EXEC_PATH`, `GIT_CONFIG_PARAMETERS`, `GIT_EXTERNAL_DIFF`,
`GIT_SSH_COMMAND`, `GIT_ASKPASS` and the others the name begins `GIT_` can each name a program or a
configuration.

`verified-by: none`

<a id="COMMIT-7"></a>
### COMMIT-7: the commit runs no program a file in the repository names, and is made only where the reader opens the repository

No program a file in the repository names is started. No hook runs, so neither do `pre-commit`,
`commit-msg`, `prepare-commit-msg`, `post-commit`, `reference-transaction` and `pre-auto-gc`, nor any
hook a configuration's `core.hooksPath` names. No filesystem monitor starts, no signing program
starts, and no editor or pager opens. git's own automatic collection and maintenance do not run after
the commit. The commit is made unsigned, whatever `commit.gpgsign` says. A path a filter or a
conversion would change is never staged ([COMMIT-3](#COMMIT-3)).

git reads the person's own configuration for an identity. The line-ending and attribute settings of the
person's global and system configuration are set aside for the start, as the reader does not read them
([GIT-10](tools/read-git.md#GIT-10)), so git converts no path the reader listed. The person's own
ignore settings apply: a path they cover is not committed, and the note names it
([COMMIT-8](#COMMIT-8)). The person's own configuration is the person's file, trusted as their hooks
are ([HOOK-1](hooks.md#HOOK-1)).

The repository gets no commit where `read_git` would not open it for the session's working directory:
the map does not vouch for every file under its `.git` ([GIT-2](tools/read-git.md#GIT-2)), or it holds
what [GIT-8](tools/read-git.md#GIT-8) declines, which includes a `.git` that is a file and
configuration that includes another file.

**Why.** git runs the programs a repository's configuration and hooks name, with nothing confining
them. A commit made by a driver that let those run would be the repository deciding what the driver
executes, which is the rule's opposite. The refusal [GIT-1](tools/read-git.md#GIT-1) makes for a read
and [CHECKOUT-5](checkouts.md#CHECKOUT-5) makes for a checkout applies to a write.

**Why unsigned.** Signing starts a program and, for a key held by an agent or a card, asks the person
something nobody is placed to answer in the middle of a turn.

**Why a linked worktree gets no commit.** The reader declines it, and the commit follows the reader so
that the driver decides nothing from files a read would not have opened.

`verified-by: none`

<a id="COMMIT-8"></a>
### COMMIT-8: the person is told what was committed and what was left out, and the trail records it

Under the turn's reply, beside any note [TURN-11](turns.md#TURN-11) describes, a line names the commit:
the first ten characters of its id, as `read_git` shows one ([GIT-1](tools/read-git.md#GIT-1)), and how
many paths it holds. A front end that draws no note under a reply puts the line where it puts its other
notes: a one-shot run writes it to stderr ([CLI-5](cli.md#CLI-5)).

A line for each path left out names it and one cause from a fixed set: `not in the repository`, `could
not be compared`, `a credential`, `held earlier changes`, `existed before the turn`, `git did not commit
it`. A path [COMMIT-4](#COMMIT-4) leaves out has that cause alone. A path the status does not list is
not named, since it is unchanged, ignored by the repository's own files or covered by a deny rule. A
path is drawn as a file name is in a vouch prompt: a control character and a character that draws as
nothing are written out ([PROMPT-13](prompting.md#PROMPT-13)).

Where nothing was committed for a reason about the repository, one line says which: `repository not
opened`, `status not available`, `HEAD is detached`, `no commit yet`, `git not found`, or `git
refused` with the exit status git ended with, or with the bound where git was ended at it. What git
printed is shown to the person as a command's output is, drawn as untrusted output, and is never put in
the conversation.

The trail ([TRACE-1](trace.md#TRACE-1)) records, under `commit`, the commit made with its id and the
number of paths it holds, each path left out with its cause, and each refusal with git's exit status.
It holds neither the message nor what git printed ([TRACE-2](trace.md#TRACE-2)).

A commit that is not made, and one that fails, do not change how the turn ended, as a hook that goes
wrong ends nothing ([HOOK-7](hooks.md#HOOK-7)). The reply is still the reply and the files are still
written. Nothing about the commit is added to the conversation, so the planner learns of it only by
reading the history.

**Why.** A commit is the one effect of a turn that outlasts a rewind of its files. Naming the id gives
the person the thing they would revert, and naming each path left out says why a file they expected in
history is not there, which is the question a commit holding fewer paths than the turn wrote raises.

**Why the cause is a fixed word.** The driver chooses it from a fixed set, so no file content and no git
output reaches the sentence.

`verified-by: none`

<a id="COMMIT-9"></a>
### COMMIT-9: taking a turn back takes its commit back, as one unbroken run the branch still stands on

`/rewind` lists, on the row of a turn that made a commit, the first ten characters of its id. `/undo`
and `/rewind <n>` put the files back as [SESSION-19](sessions.md#SESSION-19) says, and then take back
the commits of the turns they rewound.

The commits are taken back only as one unbroken run: HEAD still names the branch they went on, the
branch names the newest of them, and each has the one before it as its parent. The refusals of
[COMMIT-7](#COMMIT-7) hold as they did for the commit, so a repository the reader no longer opens has
none taken back. The branch then goes back to the parent of the oldest, and afterwards the index
entries of the paths those commits held go back to match that parent. No other index entry and no
file in the working tree changes beyond what the rewind put back. The branch moves only if it still
names the newest commit when it moves, so a commit made by another program between the check and the
move is not lost.

Where the run is broken, because the person committed since, a commit that is not the driver's lies
between two of them, or the branch was switched or moved, every commit is left where it is. The same
holds where the rewind could not put back a path the commit held, or where the move of the branch is
refused. The files still go back as far as they can, and the line that reports the rewind names each
commit it left and says why, so the person can revert them with git. Where the branch has moved and the
reset of the index then fails, the commit counts as taken back, and the line says the paths show as
staged.

After a rewind the paths a commit held match its parent, and a path that held changes before the turn,
which no commit held, is as the rewind left it.

The line that reports a rewind names each commit it took back and each it left. It names none for a
turn that made none.

**Why.** A rewind that put the files back and left the commit would leave history describing a tree
that is not there, which is the disagreement [SESSION-19](sessions.md#SESSION-19) exists to prevent. A
commit the person has built on is theirs, and moving the branch from under it would lose work, so the
driver leaves it and says so.

**Why the index.** Left as the commit put it, the paths would show in `git status` as staged changes
undoing the rewind.

`verified-by: none`

<a id="COMMIT-10"></a>
### COMMIT-10: a rewind point holds the commit its turn made, and a resume brings it back

The point a turn opens for a rewind holds the commit the turn made as the id the branch shows after it,
the branch, and the paths the commit held. The commit is recorded on the point when it is made, so a
rewind later in the same session takes it back whether or not the record has been written. A resume
brings the point back with all three, and `/undo` after the resume treats the commit as COMMIT-9 does.
The commits are taken back only where that branch names the newest id when the rewind runs and the ids
form the run COMMIT-9 requires. These fields use the local-record trust model and do not protect against
a record edited by hand, as [SESSION-22](sessions.md#SESSION-22) says of coverage.

A record written before this holds no commit and reads as a turn that made none. A record carrying one
that an earlier build reads has the field dropped when that build saves it, and its `/undo` then puts
the files back and leaves the commit.

A fork keeps no points ([SESSION-18](sessions.md#SESSION-18)) and so no commits. A `/clear`,
`/compact` or anything else that gives up every point ([SESSION-19](sessions.md#SESSION-19)) gives up
the commits with them, and leaves the commits in the repository.

**Why.** Past a resume the commit is the part of a turn a person would most want to find again, and
the file copies alone would put the tree back under a history still holding the turn.

`verified-by: none`

<a id="COMMIT-11"></a>
### COMMIT-11: every front end commits and rewinds through one shared implementation

The terminal interface, `--plain`, a one-shot run, the desktop application and an editor over ACP
make the commit of COMMIT-2 to COMMIT-8 and take it back as COMMIT-9 says, through one implementation
they share. A front end adds only where the lines COMMIT-8 describes are drawn.

A delegate that works in the turn's own workspace writes paths that are the turn's, and they are
committed with it. A manifest run is not a turn and makes no commit.

Under the sandbox mode `read-only` and in plan mode the file tools write nothing
([SANDBOX-22](sandboxing.md#SANDBOX-22), [MODE-3](permission-modes.md#MODE-3)), so the turn has no
paths and makes no commit. `--incognito` changes nothing here: the commit is in the repository,
outside the directory that mode keeps clear ([INCOG-8](incognito.md#INCOG-8)).

**Why.** A front end that carried its own copy of this would decide a commit differently from the
others, and the setting is one key in one file the person reads once.

`verified-by: none`

## What this changes in other specs

None of the following is true of a build that has no turn commit. Each is an edit to that spec made in
the change that builds this one.

- **[sessions.md](sessions.md).** [SESSION-19](sessions.md#SESSION-19) and
  [SESSION-21](sessions.md#SESSION-21) describe a rewind of files, and gain the commit it takes back
  and lists. [SESSION-22](sessions.md#SESSION-22) names what a point holds, and gains the commit, the
  branch and the paths. The Known cost that a rewind sees file-tool writes and nothing else is
  unchanged and names the same limit for a commit.
- **[backends.md](backends.md).** The table of how layers combine gains `commit` among the names a
  project or local layer cannot set.
- **[trace.md](trace.md).** [TRACE-1](trace.md#TRACE-1) gains the `commit` entries of COMMIT-8, which
  record an effect of the driver and not a gate's decision, and [TRACE-2](trace.md#TRACE-2) lists what a
  field may hold and gains a commit id and an exit status.
- **[hooks.md](hooks.md).** [HOOK-2](hooks.md#HOOK-2) says when `turn-finished` fires, and gains that
  it fires after the commit.
- **[credential-protection.md](credential-protection.md).** [CRED-16](credential-protection.md#CRED-16)
  records that a path whose change holds a finding is not committed, and the table of where a
  credential lives lists a turn's commit among what a turn writes to the git directory.
- **[tools/read-git.md](tools/read-git.md).** The repositories [GIT-8](tools/read-git.md#GIT-8)
  declines are the ones COMMIT-7 refuses, [GIT-10](tools/read-git.md#GIT-10) is what COMMIT-3 and
  COMMIT-4 ask and gains the listing of each untracked file, and the reader is what shows the planner
  the commit afterwards.
- **[sandboxing.md](sandboxing.md).** [SANDBOX-30](sandboxing.md#SANDBOX-30) lists the programs
  bravebot starts on its own account and looks up by name, and gains the `git` of a turn's commit with
  the reduced `PATH` and the names it inherits.
- **[compaction.md](compaction.md) and [goal.md](goal.md).**
  [COMPACT-15](compaction.md#COMPACT-15) and [GOAL-19](goal.md#GOAL-19) say one setting names the model
  for both side requests, and it names it for the commit's message as well.
- **[cli.md](cli.md).** [CLI-5](cli.md#CLI-5) keeps stdout to the reply, so a one-shot run writes the
  line of COMMIT-8 to stderr.
- **[incognito.md](incognito.md).** [INCOG-8](incognito.md#INCOG-8) lists what still reaches the
  filesystem, and gains a commit.
- **[checkouts.md](checkouts.md).** [CHECKOUT-17](checkouts.md#CHECKOUT-17) says a rewind leaves a
  checkout as it is, and a commit is left the same way.
- **[localization.md](localization.md).** [LOCALE-1](localization.md#LOCALE-1) says what a catalog
  holds and what it does not, and gains the fixed commit sentence among what it does not: it is
  written into a repository and not shown to the person watching.

## Not decided

- **Committing earlier changes first.** A turn that edits a path the person was already changing could
  commit those changes first, that path alone, under a fixed message, and then commit its own edit.
  COMMIT-4 leaves such a path out instead. Saving first needs a read of the status and a git start in
  the middle of the turn before each such write, puts the person's unfinished work into history, leaves
  a commit behind when the turn is stopped, and gives a rewind a second commit to take back. A person
  who wants the turn's edit committed commits the earlier changes first.
- **A second value that runs the repository's hooks.** The hooks would run inside the confinement `run`
  gives a stage. A hook that rewrites files after the commit leaves a tree that differs from it, and
  one that fails would have to be told apart from git refusing. The value's name and how a hook is
  confined are not chosen here.
- **A status of one path where the rest of the tree is not vouched for.** The reader answers a status
  only where the map vouches for the whole working tree ([GIT-11](tools/read-git.md#GIT-11)), so a
  repository holding one path nobody vouched for gets no commit and the note says `status not
  available`. A status of the one path, answered where its own bytes and the ignore and attributes
  files above it are vouched for, would commit in more repositories. Whether the reader may answer it
  is a change to [GIT-11](tools/read-git.md#GIT-11).
- **Writing the commit without starting git.** The driver writes a checkout's files, index and
  worktree entry itself ([CHECKOUT-5](checkouts.md#CHECKOUT-5)) and so starts nothing. A commit would
  need the objects, the index and the ref written the same way, for every index and ref format a
  person's repository may use, and no hook, filter or signing key would apply. COMMIT-6 starts git
  because that writer does not exist.
- **Whether a rule or a pinned file can forbid the commit.** A `deny` rule and an administrator's pin
  ([PERM-19](permissions.md#PERM-19)) are about lines `run` is asked for, and the setting is the
  person's own. A team that wants no commits on any machine has no key to pin.
- **Whether one extra request for each turn is the right price for a message.** The first line of the
  turn's prompt, cut as a session title is ([SESSION-4](sessions.md#SESSION-4)), costs nothing and
  describes the request and not the result. The planner's own last reply is already paid for and is
  not one line. How the request's spend and wait are charged to the turn is open with it.
- **The language of the planner's line.** The question names none, so the line is in the language the
  planner answers in. A repository whose history is in one language may want the line asked for in it.
- **Whether the planner is told that the driver commits.** A planner that does not know may run
  `git commit` itself, which a person is asked to approve, and the turn then ends with a second commit
  of what is left. A sentence in the standing text would also be one more thing on every round.
- **Whether a one-shot run's `--json` result carries the commit id.** [CLI-12](cli.md#CLI-12) lets a
  field be added within a schema number, and COMMIT-8 adds none.
- **A way to turn it off for one session,** such as `/commit`, and to commit changes the person made
  outside the conversation, as `aider`'s `/commit` does.
- **A style for the message.** Conventional Commits, or a prefix, as a setting. No value is proposed.
- **A mark on the author.** `aider` appends a word to the author and committer names. Here only the
  trailer `attribution` names is added, and the identity is the person's.
- **Signing.** A signed commit needs a program started and a key that nobody has to answer for in the
  middle of a turn.
- **A repository with HEAD detached or no commit yet,** and a session working in a linked worktree.
  COMMIT-2 and COMMIT-7 commit nothing there. Making the first commit, or committing on a detached
  HEAD, are not proposed.
- **A write a program made.** A line `run` started that changed files leaves nothing in the turn's
  paths ([SESSION-19](sessions.md#SESSION-19)). Committing it needs a record of what a program wrote,
  which is the larger mechanism the rewind's Known cost names.
- **A manifest run.** It writes through plans and ends with an outcome of its own
  ([manifest.md](manifest.md)), and no clause here commits one.
- **The name of the setting.** `commit` is also a key inside the `attribution` block, which names the
  text a commit message may carry. A different name for this one would not be mistaken for it.
- **How the driver learns which chosen paths git left out.** The person's own ignore settings are not
  read by the reader, so git decides, and the driver learns it from the commit's tree
  ([COMMIT-6](#COMMIT-6)).
- **A project layer for the setting.** A team that wants every contributor's sessions to commit cannot
  say so in the repository, by design.

## Known costs

- **The commit holds each path as it stands when it is staged.** An edit the person made to one of the
  turn's paths while the turn ran, or while the message was requested, is in the commit, under a message
  about the turn.
- **A program's change to a path the turn wrote is in the commit.** A formatter or a build step started
  by `run` that rewrote such a path has the rewrite committed with it, and a background job the turn
  left running can still be writing when the commit is made. A file only a program wrote is left for
  the person.
- **A path that held changes before the turn is never committed by it.** A turn that edits ten files
  the person had been changing leaves all ten uncommitted, and `held earlier changes` names each.
- **A path git ignores is not committed and is not named.** The status lists neither an ignored path nor
  one equal to HEAD, so the note cannot tell them apart.
- **A repository holding a path nobody vouched for gets no commit.** A turn that wrote untrusted data
  to a path puts the status out of reach for every turn after it, until a trusted write replaces the
  path, and a path being written while another writer's effect is in progress does the same for that
  moment ([TRUST-4](trust-map.md#TRUST-4)).
- **On Windows a repository that does not say how line endings are handled commits nothing.** The
  reader takes every text file as converted unless the repository's own configuration says otherwise
  ([GIT-10](tools/read-git.md#GIT-10)), so each path is one it does not compare.
- **No hook runs for this commit.** That includes a hook the person's own configuration names, a
  secret scanner among them. The credential scan of what a turn writes
  ([CRED-16](credential-protection.md#CRED-16)) is the check that does run.
- **A `turn-finished` hook that rewrites files runs after the commit.** A formatter's output is not in
  it, and it leaves the tree differing from HEAD.
- **The commit is unsigned.** A repository that requires signed commits needs the person to amend.
- **A global line-ending or attribute setting does not shape the commit.** A file such a setting would
  have converted is committed as the repository's own settings say.
- **The keys that start a program are the ones the build knows.** A key a later git adds is a gap until
  the build learns it.
- **A person's `GIT_` variables do not reach git.** An identity given by `GIT_AUTHOR_NAME` is not used,
  and git reads the person's configuration for one.
- **A repository that uses LFS or a filter commits no path the filter covers,** and a session in a
  linked worktree gets no commit while the reader declines one. The sessions the proposed worktree and
  background specs put in a linked worktree ([worktrees.md](worktrees.md),
  [BG-14](background-sessions.md#BG-14)) are in that case once built.
- **Every turn that commits waits for one more request,** unless the fixed message is used.
- **A turn that writes reads the status twice.** The reader hashes every changed file and reads every
  ignore and attributes file, and in a repository where it runs past its deadline
  ([GIT-9](tools/read-git.md#GIT-9)) the turn commits nothing and says `status not available`.
- **A start of git that is ended at its bound can leave `.git/index.lock`.** The person's next git
  reports it, and the paths may stay staged.
- **A rewind that finds the run broken leaves every commit.** The person reverts them with git.
- **A rewind leaves a reflog entry and the objects.** The commit stays reachable through the reflog
  until git collects it.
- **A rewind and a commit replace the person's staging of the paths they move.** The index entries of
  those paths match the commit or the branch afterwards, whatever was staged.
- **`/hunks` puts one hunk back and leaves the commit as it was.** The tree then differs from HEAD by
  that hunk ([SESSION-37](sessions.md#SESSION-37)).
- **A record an earlier build re-saves loses the commit.** Its `/undo` then leaves it.
- **The fixed message is in English,** whatever the language setting says.
- **An incognito session writes a message into the repository.** The repository is outside what the
  mode keeps clear.

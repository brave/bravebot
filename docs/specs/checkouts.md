---
id: CHECKOUT
title: A checkout of its own for a delegate
status: proposed
governs:
  - crates/agent/src/agents.rs
  - crates/agent/src/delegate.rs
  - crates/agent/src/git.rs
  - crates/agent/src/scratch.rs
  - crates/agent/src/tools.rs
  - crates/agent/src/workspace.rs
  - crates/core/src/delegate.rs
  - crates/core/src/trust.rs
documented-by: none (gap: a page on giving a delegate a checkout, owed once one can be made)
---

## Scope

A delegate that works in a checkout of its own instead of in the session's working directory: how
one is asked for, how the driver makes it without starting git, what the delegate may do in it,
how its work comes back into the person's tree, and when it is removed.

A **checkout** here is a detached linked worktree of the session's repository, holding the tree of
the commit the repository has checked out (HEAD), which the driver makes for one delegate. Other
specs use the word for the person's own clone of a project. Here that is always called the
working directory, and a checkout is always one a delegate was given.

Nothing in this file is built. It is a design, written to be agreed before the work starts, and
every clause says so.

**A checkout is not a sandbox.** It moves where a delegate's file tools reach and where its
programs start. A program it runs is as unconfined as any other ([sandboxing.md](sandboxing.md)),
so it can write the person's tree by an absolute path. What keeps the person's tree as it was is
that the file tools no longer reach it, and that is all.

What a delegate is, and what it may hold, is [delegation.md](delegation.md). Which paths are
trusted and what a write does to that record is [trust-map.md](trust-map.md). Reading a repository
without starting git is [tools/read-git.md](tools/read-git.md). Where a clause here changes one of
those, that file says so at the place it changes. What the desktop front end shows of a checkout
is not specified here.

## What exists today

Every delegate works in the session's working directory and shares its live file authority
([DELEGATE-15](delegation.md#DELEGATE-15)). Reservations are per path, so two delegates do not
interleave writes to one file. Nothing else separates them. Two workers fanned out over two
modules each build both sets of edits, wait on one build directory's lock, and can report a
failure the other one caused. A worker that stops part-way leaves its edits in the person's tree,
and a rewind puts back what the file tools wrote and nothing a formatter or code generator it ran
wrote ([SESSION-19](sessions.md#SESSION-19)).

A definition's `isolation:` key is not read ([MEMORY-7](definition-memory.md#MEMORY-7)).

## Asking for one

<a id="CHECKOUT-1"></a>
### CHECKOUT-1: `isolation` on a spawn is routing, with one value

`spawn_agent` takes `isolation`, whose one value is `checkout`. Any other value is refused, and no
delegate starts. It is routing for the reason `kind` is ([AGENT-1](tools/spawn-agent.md#AGENT-1)):
it is a name from a list the driver wrote, and it decides where the delegate's effects land. A
person could approve it on its own: "this delegate works in a new checkout of commit `abc1234` and
writes nothing into your working directory until you apply it".

Under `each`, every delegate the call starts is given a checkout of its own.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-2"></a>
### CHECKOUT-2: a definition may ask for a checkout, and a later definition cannot take one away

A definition may say `isolation: checkout`. A delegate is given a checkout where its definition
asks or the call asks, so the planner cannot drop one a definition asked for. `worktree`, the value
Claude Code reads, asks for one too. Any other value loads the definition without a checkout, and
says so, as a `memory:` value nothing here reads does ([MEMORY-2](definition-memory.md#MEMORY-2)).

Where a later definition of the same name replaces an earlier one
([DELEGATE-20](delegation.md#DELEGATE-20)), the key is met as the kind is: either one asking gives
a checkout. A project's definition can add a checkout to a person's own and cannot remove one.

On a turn a person addresses to a definition ([ADDRESS-1](addressing-a-definition.md#ADDRESS-1)),
the key is not applied, and the turn says so.

**Why a file may ask.** A definition may choose what a run is for and never what it may reach
([ADDRESS-7](addressing-a-definition.md#ADDRESS-7)). This key names no path. The checkout holds
HEAD's tree from a `.git` the map already trusts in full ([CHECKOUT-4](#CHECKOUT-4)), and a delegate
in one no longer reaches the working directory ([CHECKOUT-7](#CHECKOUT-7)), so it reaches no file
the session did not. What it causes is a write of the driver's own values into the person's
repository ([CHECKOUT-5](#CHECKOUT-5)), and whether that should ask is an open question below.

**Why `worktree` too.** A definition written for Claude Code with that value asks for the same
separation. Loading it without one would put work its author meant to keep apart in the person's
tree. Its checkout holds HEAD, where Claude Code's is branched from the default branch.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-3"></a>
### CHECKOUT-3: a checker or a worker may have a checkout, a reader may not, and a delegate's own delegates share it

A `reader` asking for a checkout is refused, and no delegate starts. A delegate in a checkout
starts its own delegates in that checkout, and asking for another checkout there is refused.

**Why not a reader.** A reader writes nothing, so a checkout separates it from nobody, and it
would be shown HEAD's tree in place of the tree the person has.

**Why one level.** A checkout made from a checkout would need its repository opened through a
linked worktree, which [GIT-8](tools/read-git.md#GIT-8) declines in the person's tree and which
[CHECKOUT-12](#CHECKOUT-12) builds only for the driver's own record of one.

Nothing builds this yet.

`verified-by: none`

## Making one

<a id="CHECKOUT-4"></a>
### CHECKOUT-4: a checkout is made only where `read_git` would open the repository, and git would write every file as it is stored

A checkout is refused, and no delegate starts, unless `read_git` would open the session's
repository: the map trusts all of its `.git` ([GIT-2](tools/read-git.md#GIT-2)) and nothing
[GIT-8](tools/read-git.md#GIT-8) declines is there. A working directory that is itself a linked
worktree is one of those.

It is also refused where git would write a file differently from the stored blob: a `filter`,
`ident` or `working-tree-encoding` attribute, `eol=crlf`, `core.autocrlf=true` or `core.eol=crlf`.
These are read from the `.gitattributes` files in HEAD's tree, from `info/attributes` and from the
configuration, and they decide only whether a checkout is refused.

It is refused where HEAD's tree holds more files or more bytes than a fixed bound the driver sets.
The sizes are read from the object store's own headers, before any file is written.

It is refused where the tree holds an entry named `.`, `..` or `.git` in any case, an entry whose
name holds a separator, or two paths the file system would take for one.

Each refusal says which of these it was.

**Why all of `.git`.** Writing a tree means following ids: a ref names a commit, the commit its
tree, the tree its entries. Following them over bytes nobody vouched for is the driver branching on
untrusted content.

**Why refuse a filter rather than run it.** A filter is a program the repository names. Running it
is running somebody else's program with nobody asked, and writing the blob unfiltered gives a file
git would not have written. A repository using LFS therefore gets no checkout.

**Why the entry names.** git refuses to check out a tree holding these. A nested `.git` would be a
repository whose configuration the person's own git reads, and two paths folded to one name would
leave one file written over another and two rules for one path ([CHECKOUT-8](#CHECKOUT-8)).

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-5"></a>
### CHECKOUT-5: a checkout is HEAD's tree written by the driver, and nothing is started to write it

No program is started to make a checkout: not git, and nothing a hook, `core.fsmonitor` or the
configuration names. The driver writes HEAD's tree file by file. A file keeps mode `100644` or
`100755`. A symbolic link is written as one where `core.symlinks` would have git write one, and as
a file holding its target otherwise. A submodule's entry becomes an empty directory, as git leaves
a submodule nobody initialised.

A path a deny rule covers ([PERM-7](permissions.md#PERM-7), [GIT-4](tools/read-git.md#GIT-4)) is
not written, and the delegate is told which paths were left out.

The driver then writes what git needs to know the checkout for a detached linked worktree:
`worktrees/<id>/` under the person's `.git`, holding the checkout's HEAD, the path back to the
common directory and the path to the checkout; a `.git` file in the checkout naming that entry;
and an index of HEAD's tree. Every byte of these is the driver's own: two paths and a commit id. No
branch is made and no ref under `refs/` changes. A person's own git, and a line a person approved
running git, work in the checkout as in any worktree, and `git worktree list` shows it.

Making a checkout asks nobody, and writes nothing into the working tree.

**Why not `git worktree add`.** git runs what the repository names as it checks a tree out: a
`post-checkout` hook, a `core.fsmonitor` program, a filter driver. `read_git` starts no program for
the same reason ([GIT-1](tools/read-git.md#GIT-1)).

**Why the person's `read_git` is unaffected.** [GIT-8](tools/read-git.md#GIT-8) never reads a ref
under `worktrees/`, and the entry changes nothing a read of the working directory opens.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-6"></a>
### CHECKOUT-6: a checkout lives under the state directory, keyed by the workspace

A checkout is made at `~/.bravebot/checkouts/<workspace key>/<id>`, keyed by the workspace as a
language server's cache is ([LSP-10](tools/lsp.md#LSP-10)). Each directory on the way is created at
the modes [STATE-1](state-directory.md#STATE-1) gives, and the checkout's own directory is created
rather than adopted: a name something already holds is refused, as the session's own directory
refuses one ([TRUST-14](trust-map.md#TRUST-14)).

A session that keeps no record ([incognito.md](incognito.md)), or a machine with no state
directory ([STATE-2](state-directory.md#STATE-2)), makes its checkouts in the system temporary
directory instead, on the terms the session's own directory is made there, under a name of a third
kind beside the session's and a local server's. They go when the session ends
([TRUST-15](trust-map.md#TRUST-15)).

**Why not the system temporary directory everywhere.** A checkout holding work nobody has applied
has to last through a reboot and a few idle days. macOS removes files there that go unused for
days, and many Linux systems clear it at boot.

**Why not inside the workspace.** Under `.git`, a file a delegate wrote nobody vouched for would
distrust a path inside `.git` and close `read_git` for the whole repository
([GIT-2](tools/read-git.md#GIT-2)). Anywhere else in the tree, it is a directory every build,
search and `git status` of the person's has to skip, and build tools in it walking up the tree
would find the person's own configuration.

Nothing builds this yet.

`verified-by: none`

## Working in one

<a id="CHECKOUT-7"></a>
### CHECKOUT-7: a delegate's workspace is its checkout

A delegate in a checkout works in a workspace whose root is the checkout. The directories the
parent had opened beside its working directory, and the session's own directory, stay reachable
as they were, and so does the rule holding the file tools to the workspace
([PERM-16](permissions.md#PERM-16)). The working directory is not reachable from it, and the
checkout is reachable from no run but that delegate and the delegates it starts.

What follows from the root follows unchanged. A command line starts at the checkout's root
([CMDLINE-12](tools/command-line.md#CMDLINE-12)), a hook runs there
([HOOK-4](hooks.md#HOOK-4)), and a relative path means a path in the checkout.

The delegate is told, in the driver's words, which commit the checkout holds, that changes the
person has not committed are not in it, and which paths a deny rule left out.

**Why reach without a person opening it.** On the grounds the session's own directory is reached
([TRUST-16](trust-map.md#TRUST-16)): it was created here, owned by this account, and holds only
what the driver wrote into it from a `.git` the map trusts in full.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-8"></a>
### CHECKOUT-8: a file in a checkout is labelled as the same path in the working directory is

As a checkout is made, each rule the map holds under the working directory is copied to the same
relative path under the checkout, and the answer the workspace was given at startup
([TRUST-7](trust-map.md#TRUST-7)) answers for every path no copied rule names. Re-spelling the map
for a new root, as `/cd` does ([TRUST-13](trust-map.md#TRUST-13)), is not that: every rule would
still name the file it named, and none would answer for a path in the checkout.

The copied rules join the session's live file authority as rules outside the project. A write in
the checkout is then recorded under the checkout's path ([TRUST-4](trust-map.md#TRUST-4),
[TRUST-5](trust-map.md#TRUST-5)), comes back to the session as any delegate's decision does
([DELEGATE-11](delegation.md#DELEGATE-11)), and is kept in the session record in full, as a rule
outside the project is ([TRUST-6](trust-map.md#TRUST-6)). A checkout's rules go when it is
removed.

`~/.bravebot` is read as trusted by provenance ([TRUST-11](trust-map.md#TRUST-11)). A checkout is
not: it holds a project's files, and is read through the map whatever directory it is in.

**Why the path's own rule.** `read_git show HEAD:<path>` is labelled by all of `.git` and by the
rule over `<path>` ([GIT-3](tools/read-git.md#GIT-3)), and when a checkout is made its file at
`<path>` holds exactly those bytes. With all of `.git` trusted ([CHECKOUT-4](#CHECKOUT-4)), the
meet of the two is the path's own rule.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-9"></a>
### CHECKOUT-9: permission rules hold in a checkout as in the working directory, and nothing in a checkout is read as configuration

A rule's relative specifier ([PERM-3](permissions.md#PERM-3)) is held against the checkout's
root, so a rule denying a read of `.env` refuses a read of `<checkout>/.env`. An absolute
specifier under the working directory is copied to the checkout as the trust rules are. What a
person answered about a credential a read turned up
([CRED-15](credential-protection.md#CRED-15)) is not copied, as a move with `/cd` drops it
([TRUST-13](trust-map.md#TRUST-13)).

Nothing in a checkout is read as a source: not its `.bravebot/settings.json`
([PERM-15](permissions.md#PERM-15)), not its `.bravebot/agents/`
([DELEGATE-20](delegation.md#DELEGATE-20)) and not its `AGENTS.md`. The delegate has the
working directory's, as they were resolved before the turn.

**Why.** A checkout is the same tree at HEAD, and a rule written about the project is a rule about
it. A source read from it would be a second copy of the project's configuration, which a delegate
writing there could change for the delegates it starts.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-10"></a>
### CHECKOUT-10: a command vouched for in the working directory is asked about again in a checkout

A line remembered as vouched for is keyed by the tree it was given in
([RUN-8](tools/run.md#RUN-8)), and a checkout is not that tree. A run in a checkout is asked about
unless an entry names the checkout.

**Why.** `sh check.sh` vouched for at the root is a statement about the file the root holds, and
the checkout's file at that path may differ.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-11"></a>
### CHECKOUT-11: a write in a checkout asks what the same write in the working directory asks

A path in a checkout takes every gate the same path in the working directory takes, and asks the
same question. The session's own directory is held to the same rule
([TRUST-16](trust-map.md#TRUST-16)): a place to write is not a place writes stop being asked about.

Bringing the file back asks again ([CHECKOUT-14](#CHECKOUT-14)). Whether the first question can go
is an open question below.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-12"></a>
### CHECKOUT-12: `read_git` in a checkout is routed by the driver's record, never by the checkout's `.git`

A question about the checkout's repository is answered from the common directory and the
`worktrees/<id>/` entry the driver recorded when it made the checkout. `<checkout>/.git` is never
read. The files a read opens are the entry's HEAD and index beside the common directory's own, and
[GIT-2](tools/read-git.md#GIT-2)'s rules are held against that list. A status
([GIT-11](tools/read-git.md#GIT-11)) is then answered in the checkout on its usual terms.

**Why not the file.** A delegate can overwrite it, and what it names would then choose which
repository is read.

Nothing builds this yet.

`verified-by: none`

## Bringing work back

<a id="CHECKOUT-13"></a>
### CHECKOUT-13: the paths that can come back are the ones the driver recorded, and the ones a trusted status lists

A checkout's candidate paths are, first, every path the driver recorded a file effect on in it: a
`write_file`, an `edit_file`, and a redirection a command line wrote through
([CMDLINE-5](tools/command-line.md#CMDLINE-5)). A planner holding nothing untrusted chose those, so
they are routing. Second, where a status over the checkout would be answered
([GIT-11](tools/read-git.md#GIT-11)), the paths it lists. Where it would not, a file a program
wrote is not found, and wherever the candidates are given they say the status could not be read.

No path is a candidate for differing from HEAD on a comparison of bytes the driver made. A path a
deny rule covers is never one.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-14"></a>
### CHECKOUT-14: a file comes back as a write through the gate, one path at a time

`apply_checkout` takes two routing fields: `delegate`, the number of a checkout the session keeps,
and an optional `paths`, a subset of that checkout's candidates. Without `paths` it takes every
candidate. A number the session keeps no checkout for, or a path that is not a candidate, is
refused.

Each path is a write of the checkout file's bytes to the same path in the working directory,
through the gate a `write_file` with `contents_ref` takes ([tools/write-file.md](tools/write-file.md)):
it is always shown ([WRITE-4](tools/write-file.md#WRITE-4)), a person's yes is bound to that path
alone ([WRITE-3](tools/write-file.md#WRITE-3)), and the map, the permission rules, the permission
mode and a rewind treat it as any write. The bytes keep the label the checkout's path gives them,
so a file a write left untrusted there is still untrusted once it has come back, and the
destination is recorded as [TRUST-4](trust-map.md#TRUST-4)'s table says.

The person is shown the difference between their file as it is and the checkout's. Where the
driver's record holds a write to that path in the working directory after the checkout was made,
the prompt says so. The driver does not decide whether the person's file changed in between, since
that is a comparison of bytes.

A path the status lists as removed is named in the result, and is not removed.

`/checkouts apply` is the same operation, typed by a person, and asks the same questions.

**Why not a merge.** A git merge would put bytes in the person's tree with nothing recorded in the
map. That is the cost the map already carries for a file another process drops into a trusted
directory, and this program would be paying it on purpose.

Nothing builds this yet.

`verified-by: none`

## How long one lasts

<a id="CHECKOUT-15"></a>
### CHECKOUT-15: a checkout nothing was done in goes with its delegate, and any other is kept until a person removes it

Whether a checkout is kept is decided from the driver's record, never by comparing its files with
HEAD. Where the delegate recorded no file effect there and started no program there, a hook among
them, the checkout is removed as the delegate ends, with its `worktrees/<id>/` entry and its rules,
and nobody is asked: the record says it holds only what the driver wrote.

Any other checkout is kept. The session record holds its path, its commit and its delegate's
number ([SESSION-3](sessions.md#SESSION-3)), and leaving the session names each one kept
([SESSION-8](sessions.md#SESSION-8)).

`/checkouts` lists the checkouts the session keeps, shows one's candidates against the working
directory, and removes one. Removing one that has candidates asks first.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-16"></a>
### CHECKOUT-16: a resume brings kept checkouts back, a fork does not, and an opening session removes what no session lists

`--resume` brings back the checkouts the record kept, with their rules. A fork does not carry them
([SESSION-18](sessions.md#SESSION-18)), so no two records list one directory. `/cd` is refused
while the session keeps a checkout, and names it, since the record that lists it would move with
the session ([SESSION-13](sessions.md#SESSION-13)) and the checkout would stay keyed under the
directory it left.

On Unix a session holds a lock on each checkout it keeps for as long as it runs. Opening a session
removes a checkout under its workspace's key that no session record there lists and no running
session holds, with its `worktrees/<id>/` entry, on the terms the session's own directories are
swept ([TRUST-25](trust-map.md#TRUST-25)). On Windows nothing is removed, and `/checkouts` names a
checkout no record lists and leaves it to the person.

**Why a lock as well as the record.** A session opening beside one that is running would otherwise
take a checkout made since that session last wrote its record for a leftover.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-17"></a>
### CHECKOUT-17: a rewind leaves a checkout as it is, and says so

A rewind ([SESSION-19](sessions.md#SESSION-19)) puts back nothing in a checkout and records the
gap in its coverage, as it does for the session's own directory
([TRUST-19](trust-map.md#TRUST-19)). A file brought back from one is a write in the working
directory and is rewound as any write is. A delegate still ends with the turn that started it
([DELEGATE-17](delegation.md#DELEGATE-17)), so nothing runs in a kept checkout once its turn is
over.

Nothing builds this yet.

`verified-by: none`

## What else changes

<a id="CHECKOUT-18"></a>
### CHECKOUT-18: the driver names the checkout beside the report

Beside a delegate's report ([DELEGATE-8](delegation.md#DELEGATE-8)), the driver says in its own
words where the delegate's checkout is, the commit it holds, and its candidate paths, or that it was
removed. This is the one thing other than the report that crosses back
([DELEGATE-9](delegation.md#DELEGATE-9)), and all of it is the driver's record.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-19"></a>
### CHECKOUT-19: the trail records each checkout made, applied from and removed

Each is an event with the checkout's path and the delegate's number. The commit is not in it, since
the trail's fields are gate names, capabilities, labels, paths, hosts and slot ids
([TRACE-2](trace.md#TRACE-2)). The session record holds the commit.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-20"></a>
### CHECKOUT-20: a delegate in a checkout is offered no `lsp`

The session's language servers are rooted at the working directory and shared with its delegates
([LSP-8](tools/lsp.md#LSP-8)), so a path in a checkout is outside the workspace for them
([LSP-4](tools/lsp.md#LSP-4)). A delegate in a checkout is not offered `lsp`, and the answer to the
spawn says so.

**Why not a server per checkout.** It costs a second index and a second approval
([LSP-5](tools/lsp.md#LSP-5)), and is left for a later change.

Nothing builds this yet.

`verified-by: none`

<a id="CHECKOUT-21"></a>
### CHECKOUT-21: `/status` lists the session's checkouts

Each kept checkout is a line of its own, with its path, its commit and its delegate's number, as
the session's own directory has one ([TRUST-14](trust-map.md#TRUST-14)). A checkout carries rules,
so it is in the rules in force as well ([TRUST-12](trust-map.md#TRUST-12)).

Nothing builds this yet.

`verified-by: none`

## Roads not taken

- **The driver runs `git worktree add`.** git runs the programs the repository names
  ([CHECKOUT-5](#CHECKOUT-5)).
- **A copy of the working tree**, by a copy-on-write clone or a plain copy. It would carry the
  person's uncommitted changes. It also walks names nobody vouched for with no bound on size,
  skipping `target/` or `node_modules` would be a decision made by names in a tree nobody vouched
  for, and the programs a delegate runs would have no repository.
- **Files with no `.git`.** Nothing would be written into the person's repository, but git fails
  in the checkout, and so does any build that asks git for a commit id.
- **A clone sharing objects through `alternates`.** Nothing would be written into the person's
  `.git`, but [GIT-8](tools/read-git.md#GIT-8) declines alternates, and a `git gc` in the person's
  repository can prune objects the clone needs.
- **A checkout inside the workspace**, under `.git` or a hidden directory
  ([CHECKOUT-6](#CHECKOUT-6)).
- **The system temporary directory in every session** ([CHECKOUT-6](#CHECKOUT-6)).

## Open questions

- **Whether making a checkout asks.** This spec says no, since it writes nothing into the working
  tree. The case for asking is the entry under `.git/worktrees/` and the disk a checkout takes.
- **The second question on a write.** A worker's write into a checkout asks, and bringing it back
  asks again. [WRITE-3](tools/write-file.md#WRITE-3) asks because the wrong file destroys work, and
  that does not hold for a copy nobody else uses. A later change could let the spawn answer for
  writes inside the checkout and leave bringing them back as the one question.
- **One prompt for an apply.** A person's yes is bound to one path, so bringing back twenty files is
  twenty questions. One prompt showing every difference could bind one yes per path it showed, as
  the question [PERM-15](permissions.md#PERM-15) asks names every rule it grants.
- **Uncommitted changes.** A checkout holds HEAD alone. Carrying the working tree's difference into
  it is possible only where [GIT-11](tools/read-git.md#GIT-11) trusts the whole working tree.
- **A session whose working directory is a linked worktree.** [GIT-8](tools/read-git.md#GIT-8)
  declines it, so it gets no checkout. Opening a linked worktree from the common directory would
  answer that, and [CHECKOUT-12](#CHECKOUT-12) builds half of it.
- **Removals on apply.** Bringing back a removal needs a removal under the write gate, which no tool
  has.
- **Whether a desktop session gives checkouts before it can show them.** The desktop needs to list,
  show and apply them, and has an issue of its own to write.

## Known costs

- **A program in a checkout can still write the person's tree.** It is unconfined, and an absolute
  path reaches the working directory. A program in the working directory, or in another checkout,
  can write into this one the same way, and a checkout the record shows nothing was done in is
  removed with whatever such a program put there. A `run` profile ([sandboxing.md](sandboxing.md))
  would put the checkout's root where the working directory's is in a plan's write set.
- **A repository with a filter or an end-of-line conversion gets no checkout**, and that includes
  every repository using LFS.
- **A worker's first build is asked about.** A command vouched for in the working directory is not
  vouched for in a checkout ([CHECKOUT-10](#CHECKOUT-10)).
- **A write is asked about twice**, once in the checkout and once as it comes back.
- **git in a checkout reports a path a deny rule left out as removed.** The index holds HEAD's tree
  and the file is not there. Such a path is never a candidate, so nothing reaches the person's file.
- **A file a program wrote is not found where a status is not answered.** The driver's record holds
  only what the file tools and redirections wrote ([CHECKOUT-13](#CHECKOUT-13)).
- **No `lsp` in a checkout** ([CHECKOUT-20](#CHECKOUT-20)).
- **In a session that keeps nothing, a checkout goes with the session**, with whatever was not
  brought back.
- **A checker that ran a program keeps its checkout**, since it started a program there
  ([CHECKOUT-15](#CHECKOUT-15)), and a person removes it.
- **On Windows a killed session's checkouts stay** until a person removes them
  ([CHECKOUT-16](#CHECKOUT-16)).

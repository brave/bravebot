---
id: CHECKOUT
title: A checkout of its own for a delegate
status: proposed
governs:
  - crates/agent/src/agents.rs
  - crates/agent/src/delegate.rs
  - crates/agent/src/git.rs
  - crates/agent/src/git/checkout.rs
  - crates/agent/src/scratch.rs
  - crates/agent/src/tools.rs
  - crates/agent/src/workspace.rs
  - crates/agent/src/turn.rs
  - crates/core/src/delegate.rs
  - crates/core/src/file_authority.rs
  - crates/core/src/policy.rs
  - crates/core/src/trust.rs
  - crates/tui/src/app.rs
  - crates/tui/src/checkouts_command.rs
  - crates/tui/src/status.rs
documented-by:
  - docs/website/docs/customize/agents.md
  - none (gap: a page on a spawn asking for a checkout, bringing its work back and removing it, owed until the design is built)
---

## Scope

A delegate that works in a checkout of its own instead of in the session's working directory: how
one is asked for, how the driver makes it without starting git, what the delegate may do in it,
how its work comes back into the person's tree, and when it is removed.

A **checkout** here is a detached linked worktree of the session's repository, holding the tree of
the commit the repository has checked out (HEAD), which the driver makes for one delegate. Other
specs use the word for the person's own clone of a project. Here that is always called the
working directory, and a checkout is always one a delegate was given.

Part of this file is built. It is a design, written to be agreed before the work starts, and each
clause says how much of it is built. A spawn can ask for a checkout ([CHECKOUT-1](#CHECKOUT-1)),
the driver makes it ([CHECKOUT-4](#CHECKOUT-4), [CHECKOUT-5](#CHECKOUT-5)), the delegate works in
it ([CHECKOUT-7](#CHECKOUT-7)), and a checkout nothing was done in is removed
([CHECKOUT-15](#CHECKOUT-15)), and a definition can ask for one ([CHECKOUT-2](#CHECKOUT-2)).
The driver records what was written in a checkout, and beside the report names the paths the
planner typed and counts the writes made through a reference ([CHECKOUT-13](#CHECKOUT-13),
[CHECKOUT-18](#CHECKOUT-18)), `/status` lists each checkout the session has
([CHECKOUT-21](#CHECKOUT-21)), and `/checkouts` lists the ones kept and removes one
([CHECKOUT-15](#CHECKOUT-15)). `apply_checkout` brings the files a delegate wrote back, one at a
time ([CHECKOUT-14](#CHECKOUT-14)). `/checkouts apply` and keeping checkouts across a resume are
not built.

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

A spawn can ask for a checkout ([CHECKOUT-1](#CHECKOUT-1)), and so can a definition's `isolation:`
key ([CHECKOUT-2](#CHECKOUT-2)). The planner is told the paths the delegate typed for what it wrote in a kept
checkout ([CHECKOUT-18](#CHECKOUT-18)), and `/status` lists each checkout the session has
([CHECKOUT-21](#CHECKOUT-21)). The planner brings the files it names back into the working
directory with `apply_checkout` ([CHECKOUT-14](#CHECKOUT-14)), and a checkout a delegate wrote in
stays where it is until a person removes it with `/checkouts remove`
([CHECKOUT-15](#CHECKOUT-15)).

## Asking for one

<a id="CHECKOUT-1"></a>
### CHECKOUT-1: `isolation` on a spawn is routing, with one value

`spawn_agent` takes `isolation`, whose one value is `checkout`. Any other value is refused, and no
delegate starts. It is routing for the reason `kind` is ([AGENT-1](tools/spawn-agent.md#AGENT-1)):
it is a name from a list the driver wrote, and it decides where the delegate's effects land. A
person could approve it on its own: "this delegate's file tools work in a new checkout of commit
`abc1234`, and what they write reaches your working directory only as you bring it back".

Under `each`, every delegate the call starts is given a checkout of its own.

Built.

`verified-by: bravebot_agent::turn::a_delegate_given_a_checkout_writes_there_and_not_in_the_working_directory`
`verified-by: bravebot_agent::turn::a_spawn_asking_for_a_checkout_it_may_not_have_starts_nothing`

<a id="CHECKOUT-2"></a>
### CHECKOUT-2: a definition may ask for a checkout, and a later definition cannot take one away

A definition may say `isolation: checkout`. A delegate is given a checkout where its definition
asks or the call asks, so the planner cannot drop one a definition asked for. `worktree`, the value
Claude Code reads, asks for one too. Any other value loads the definition without a checkout, and
says so, as a `memory:` value nothing here reads does ([MEMORY-2](definition-memory.md#MEMORY-2)).
A definition asking is refused where a call asking would be, in a checkout
([CHECKOUT-3](#CHECKOUT-3)), with no state directory ([CHECKOUT-6](#CHECKOUT-6)) or where no
checkout can be made of the working directory, and no delegate starts. The refusal names the
definition, since the call that met it may not have asked. The planner is told, beside each
definition's description, which ones work in a checkout, since such a delegate's report is about
the last commit and not the tree the planner may have just edited.

Where a later definition of the same name replaces an earlier one
([DELEGATE-20](delegation.md#DELEGATE-20)), the key is met as the kind is: either one asking gives
a checkout. A project's definition can add a checkout to a person's own and cannot remove one, and
one given a checkout it did not ask for is told so, as one narrowed is. Where the definition that
results is a `reader`, it loads without a checkout and says so, so a project's file cannot make a
person's reader one that is refused at every spawn ([CHECKOUT-3](#CHECKOUT-3)).

On a turn a person addresses to a definition ([ADDRESS-1](addressing-a-definition.md#ADDRESS-1)),
the key is not applied, and the turn says so. That turn is then the only run of the definition to
keep its memory, since a delegate in a checkout keeps none ([CHECKOUT-9](#CHECKOUT-9)), and a
definition keeping one and asking for a checkout says so when it loads.

**Why a file may ask.** A definition may choose what a run is for and never what it may reach
([ADDRESS-7](addressing-a-definition.md#ADDRESS-7)). This key names no path. The checkout holds
HEAD's tree from a `.git` the map already trusts in full ([CHECKOUT-4](#CHECKOUT-4)), and a delegate
in one no longer reaches the working directory ([CHECKOUT-7](#CHECKOUT-7)), so it reaches no file
the session did not. What it causes is a write of the driver's own values into the person's
repository ([CHECKOUT-5](#CHECKOUT-5)), and whether that should ask is an open question below.

**Why `worktree` too.** A definition written for Claude Code with that value asks for the same
separation. Loading it without one would put work its author meant to keep apart in the person's
tree. Its checkout holds HEAD, where Claude Code's is branched, by default, from the default
branch.

**Why refused rather than dropped.** A delegate started without the checkout its definition asked
for works in the person's tree, which is what its author wrote the line to prevent.

Built.

`verified-by: bravebot_core::delegate::a_later_definition_can_give_a_checkout_and_cannot_take_one_away`
`verified-by: bravebot_core::delegate::a_reader_is_given_no_checkout_whatever_a_definition_asks`
`verified-by: bravebot_agent::agents::a_definition_asking_for_a_checkout_or_a_worktree_is_given_one`
`verified-by: bravebot_agent::agents::an_isolation_value_asking_for_nothing_here_loads_without_a_checkout_and_says_so`
`verified-by: bravebot_agent::agents::a_reader_asking_for_a_checkout_is_told_it_has_none`
`verified-by: bravebot_agent::agents::a_replacement_given_a_checkout_is_told_so`
`verified-by: bravebot_agent::agents::a_project_replacement_keeps_the_checkout_and_a_reader_is_told_it_has_none`
`verified-by: bravebot_agent::turn::a_definition_asking_for_a_checkout_gives_its_delegate_one_the_spawn_did_not_ask_for`
`verified-by: bravebot_agent::turn::a_definition_asking_for_a_checkout_inside_one_is_refused`
`verified-by: bravebot_agent::turn::a_definition_asking_for_a_checkout_with_no_state_directory_is_refused`
`verified-by: bravebot_agent::turn::a_definition_asking_for_a_checkout_outside_a_repository_is_refused_by_name`
`verified-by: bravebot_agent::tools::a_definition_asking_for_a_checkout_is_offered_as_working_in_one`
`verified-by: bravebot_agent::turn::an_addressed_turn_works_in_the_working_directory_and_says_its_checkout_is_not_applied`
`verified-by: bravebot_agent::agents::a_definition_keeping_a_memory_in_a_checkout_is_told_only_an_addressed_turn_keeps_it`
`verified-by: bravebot_agent::agents::a_definition_keeping_a_memory_and_asking_for_a_checkout_says_so_when_it_loads`

<a id="CHECKOUT-3"></a>
### CHECKOUT-3: a checker or a worker may have a checkout, a reader may not, and a delegate's own delegates share it

A spawn asking for a `reader` in a checkout is refused, and no delegate starts. A delegate in a
checkout starts its own delegates in that checkout, readers among them, and asking for another
checkout there is refused.

**Why not a reader.** A reader writes nothing, so a checkout of its own separates it from nobody,
and it would be shown HEAD's tree in place of the tree the person has. A reader a delegate in a
checkout starts reads that checkout, which is the tree the delegate that started it is working in.

**Why one level.** A checkout made from a checkout would need its repository opened through a
linked worktree, which [GIT-8](tools/read-git.md#GIT-8) declines in the person's tree and which
[CHECKOUT-12](#CHECKOUT-12) builds only for the driver's own record of one.

Built. A delegate in a checkout runs with the checkout as its workspace, so the delegates it starts
share it.

`verified-by: bravebot_agent::turn::a_spawn_asking_for_a_checkout_it_may_not_have_starts_nothing`
`verified-by: bravebot_agent::turn::a_delegate_in_a_checkout_is_refused_another`
`verified-by: bravebot_agent::workspace::a_checkout_is_not_made_from_a_checkout`

## Making one

<a id="CHECKOUT-4"></a>
### CHECKOUT-4: a checkout is made only where `read_git` would open the repository, and git would write every file as it is stored

A checkout is refused, and no delegate starts, unless `read_git` would open the session's
repository: the map trusts all of its `.git` ([GIT-2](tools/read-git.md#GIT-2)) and nothing
[GIT-8](tools/read-git.md#GIT-8) declines is there. A working directory that is itself a linked
worktree is one of those.

It is also refused where git would write a file differently from the stored blob: a `filter`,
`ident` or `working-tree-encoding` attribute, `eol=crlf`, `core.autocrlf=true` or `core.eol=crlf`,
and on Windows a `text` attribute unless `core.eol=lf`. These are read from the `.gitattributes`
files in HEAD's tree, in whatever case each is named, as a file system that ignores case opens it,
from `info/attributes` and from the repository's own `config`, and they decide only whether a
checkout is refused. `config.worktree` is not read: it is the main worktree's, and git in a linked
worktree reads that worktree's own. Every line of an attributes file counts, whichever paths
its pattern matches. A `.gitattributes` file a deny rule covers, or one the map does not trust,
refuses the checkout without being read. Its contents carry its path's label
([GIT-3](tools/read-git.md#GIT-3)), so deciding from them where that label is not trusted would be
the driver branching on untrusted content. Configuration naming an attributes file elsewhere
through `core.attributesFile` or `attr.tree` refuses it too, as a status declines it. The global
and system configuration are not read, as a status does not read them
([GIT-10](tools/read-git.md#GIT-10)), so on Windows, where Git for Windows sets `core.autocrlf` in
the system file, a checkout is refused unless the repository's own configuration sets it to
`false`.

It is refused where HEAD's tree holds more than 100,000 files, more than 100,000 directories or
more than 2 GiB, a bound the driver fixes. Directories count because a tree may name one subtree
many times, and each name is walked. The sizes are read from the object store's own headers, before
any file is written and before an attributes file is read, so a tree past a bound is refused for
that bound whatever an attributes file in it sets.

It is refused where the tree holds an entry named `.` or `..`, an entry whose name holds a
separator, an entry git's own checks take for `.git` (in any case, as NTFS reads `git~1` or a name
ending in a dot or a space, or as HFS+ reads one holding a character it ignores), a path that is
not UTF-8, which no permission rule can be held against, or two paths the file system would take
for one, which takes in a tree naming one entry twice. On Windows it is also refused for a name
Git for Windows refuses: one holding a control character or one of `<>:"|?*`, which takes in a
drive prefix such as `C:x`, one ending in a dot or a space, or a device name such as `CON` or
`LPT1`.

Each refusal says which of these it was.

**Why all of `.git`.** Writing a tree means following ids: a ref names a commit, the commit its
tree, the tree its entries. Following them over bytes nobody vouched for is the driver branching on
untrusted content.

**Why refuse a filter rather than run it.** A filter is a program the repository names. Running it
is running somebody else's program with nobody asked, and writing the blob unfiltered gives a file
git would not have written. A repository using LFS therefore gets no checkout.

**Why the entry names.** git refuses to check out a tree holding these. A nested `.git` would be a
repository whose configuration the person's own git reads, and two paths folded to one name would
leave one file written over another and two rules for one path ([CHECKOUT-8](#CHECKOUT-8)). On
Windows a path joined onto a drive prefix replaces the checkout's own path.

Built, and a spawn is refused where a checkout is.

`verified-by: bravebot_agent::workspace::a_checkout_is_made_only_of_a_repository_read_git_would_open`
`verified-by: bravebot_agent::git::an_attribute_or_setting_git_converts_by_refuses_the_checkout`
`verified-by: bravebot_agent::git::an_attribute_git_writes_a_file_as_stored_by_is_accepted`
`verified-by: bravebot_agent::git::an_attributes_file_a_deny_rule_covers_refuses_the_checkout`
`verified-by: bravebot_agent::git::an_attributes_file_the_map_does_not_trust_refuses_the_checkout_unread`
`verified-by: bravebot_agent::workspace::an_attributes_file_the_map_does_not_trust_refuses_a_checkout`
`verified-by: bravebot_agent::git::a_tree_past_the_bound_refuses_the_checkout`
`verified-by: bravebot_agent::git::a_name_git_refuses_to_check_out_refuses_the_checkout`
`verified-by: bravebot_agent::git::two_paths_the_file_system_takes_for_one_refuse_the_checkout`
`verified-by: bravebot_agent::git::a_tree_naming_an_entry_twice_refuses_the_checkout`
`verified-by: bravebot_agent::git::a_name_windows_cannot_hold_is_refused_on_windows_alone`
`verified-by: bravebot_agent::git::directories_count_against_the_file_bound`
`verified-by: bravebot_agent::git::an_attributes_file_past_the_byte_bound_is_not_read`
`verified-by: bravebot_agent::git::a_tree_past_a_bound_is_refused_for_it_before_an_attributes_file_is_read`
`verified-by: bravebot_agent::git::an_attributes_file_named_in_another_case_is_taken_for_one`
`verified-by: bravebot_agent::git::a_setting_in_config_worktree_does_not_refuse_the_checkout`

<a id="CHECKOUT-5"></a>
### CHECKOUT-5: a checkout is HEAD's tree written by the driver, and nothing is started to write it

No program is started to make a checkout: not git, and nothing a hook, `core.fsmonitor` or the
configuration names. The driver writes HEAD's tree file by file. A file keeps mode `100644` or
`100755`. A symbolic link is written as one where `core.symlinks` would have git write one, and as
a file holding its target otherwise. A link's target carries its own path's label, so whether the
link can be written does not rest on it: a NUL, which no link holds, is written as `_`, and on
Windows bytes that are not UTF-8 as U+FFFD. A submodule's entry becomes an empty directory, as git leaves
a submodule nobody initialised.

A path a deny rule covers ([PERM-7](permissions.md#PERM-7), [GIT-4](tools/read-git.md#GIT-4)) is
not written, and the delegate is told which paths were left out.

The driver then writes what git needs to know the checkout for a detached linked worktree:
`worktrees/<id>/` under the person's `.git`, holding the checkout's HEAD, the path back to the
common directory and the path to the checkout; a `.git` file in the checkout naming that entry;
and an index of HEAD's tree, in which each path left out carries git's skip-worktree flag, so git
in the checkout neither reports it removed nor commits its removal. Every byte of these is the
driver's own: two paths, a commit id, and the index of a tree the map trusts. No branch is made and
no ref under `refs/` changes. A person's own git, and a line a person approved running git, work in
the checkout as in any worktree, and `git worktree list` shows it.

Branches, tags, remote-tracking refs and `refs/stash` are in the common directory, so a checkout
shares them with the working directory and with every other checkout. A `git fetch` in one updates
a remote-tracking branch such as `upstream/main` in all of them, and fetches in several at once
fail on each other's ref locks. A `git stash pop` in one applies whatever was stashed last in any
of them, which can be the person's own entry, and drops it unless applying it conflicts.
[CHECKOUT-7](#CHECKOUT-7) says who is told.

A checkout whose directory or `worktrees/<id>/` entry already exists is refused, and what is there
is left as it was. A `worktrees` directory that is a link is refused before anything is written,
as the entry would be written wherever the link points. Two paths the file system takes for one are
found as the second is written, and a checkout refused while writing is removed with its entry.

Each checkout takes a number of its own, `c1`, `c2` and on through the session, which a resume
keeps. A delegate's number will not do: it is a path from the turn
([DELEGATE-13](delegation.md#DELEGATE-13)), so every turn has its own `d1`.

Making a checkout asks nobody, and writes nothing into the working tree.

**Why not `git worktree add`.** git runs what the repository names as it checks a tree out: a
`post-checkout` hook, a `core.fsmonitor` program, a filter driver. `read_git` starts no program for
the same reason ([GIT-1](tools/read-git.md#GIT-1)).

**Why the entry leaves the person's `read_git` as it was.** [GIT-8](tools/read-git.md#GIT-8)
never reads a ref under `worktrees/`, and the entry changes nothing a read of the working directory
opens. What a program in a checkout later commits is in the repository the two share, and
[CHECKOUT-12](#CHECKOUT-12) labels it.

Built. The writing returns the paths it left out, and the delegate is told them
([CHECKOUT-7](#CHECKOUT-7)). The session numbers its checkouts from 1. A resume does not keep the
numbers, since no session record holds a checkout yet ([CHECKOUT-15](#CHECKOUT-15)).

`verified-by: bravebot_agent::git::a_checkout_writes_heads_tree_as_a_detached_linked_worktree`
`verified-by: bravebot_agent::git::git_reads_the_checkout_as_a_clean_detached_worktree`
`verified-by: bravebot_agent::git::a_path_a_deny_rule_covers_is_left_out_and_marked_skip_worktree`
`verified-by: bravebot_agent::workspace::a_file_a_deny_rule_covers_is_left_out_of_a_checkout`
`verified-by: bravebot_agent::git::a_checkout_starts_no_program_the_repository_names`
`verified-by: bravebot_agent::git::a_checkout_where_one_exists_is_refused_and_leaves_it_alone`
`verified-by: bravebot_agent::git::a_refusal_while_writing_removes_only_what_the_checkout_made`
`verified-by: bravebot_agent::git::a_worktrees_link_declines_the_checkout`
`verified-by: bravebot_agent::git::a_link_target_holding_a_nul_is_written_with_an_underscore`
`verified-by: bravebot_agent::workspace::each_checkout_is_a_numbered_workspace_under_the_state_directory`

<a id="CHECKOUT-6"></a>
### CHECKOUT-6: a checkout lives under the state directory, keyed by the workspace

A checkout is made at `~/.bravebot/checkouts/<workspace key>/<id>`, keyed by the workspace as a
language server's cache is ([LSP-10](tools/lsp.md#LSP-10)). Each directory on the way, and every
directory in the checkout, is created at the mode [STATE-1](state-directory.md#STATE-1) gives, and
the checkout's own directory is created rather than adopted: a name something already holds is
refused, as the session's own directory refuses one ([TRUST-14](trust-map.md#TRUST-14)). A file is
written readable by its owner alone, `0600`, or `0700` where the tree marks it executable, since the
owner's execute bit is the one git compares.

A session that keeps no record ([incognito.md](incognito.md)), or a machine with no state
directory ([STATE-2](state-directory.md#STATE-2)), makes its checkouts in the system temporary
directory instead, on the terms the session's own directory is made there, under a name of a third
kind beside the session's and a local server's. They go when the session ends
([TRUST-15](trust-map.md#TRUST-15)), with their `worktrees/<id>/` entries. A session killed
outright leaves its entries in the person's `.git`, naming a checkout that is gone.

**Why not the system temporary directory everywhere.** A checkout holding work nobody has applied
has to last through a reboot and a few idle days. macOS removes files there that go unused for
days, and many Linux systems clear it at boot.

**Why not inside the workspace.** Under `.git`, a file a delegate wrote nobody vouched for would
distrust a path inside `.git` and close `read_git` for the whole repository
([GIT-2](tools/read-git.md#GIT-2)). Anywhere else in the tree, it is a directory every build,
search and `git status` of the person's has to skip, and build tools in it walking up the tree
would find the person's own configuration.

Built where the session has a state directory. A session that keeps no record, or has no state
directory, is refused a checkout and told so: the system temporary directory variant is not built.
The one test asking from a session that has none asks through a definition
([CHECKOUT-2](#CHECKOUT-2)).

`verified-by: bravebot_agent::workspace::each_checkout_is_a_numbered_workspace_under_the_state_directory`
`verified-by: bravebot_agent::workspace::a_checkout_is_keyed_by_the_workspace_and_readable_by_its_owner_alone`
`verified-by: bravebot_agent::turn::a_definition_asking_for_a_checkout_with_no_state_directory_is_refused`

## Working in one

<a id="CHECKOUT-7"></a>
### CHECKOUT-7: a delegate's workspace is its checkout

A delegate in a checkout works in a workspace whose root is the checkout. The directories the
parent had opened beside its working directory, and the session's own directory, stay reachable
as they were, and so does the rule holding the file tools to the workspace
([PERM-16](permissions.md#PERM-16)). The working directory is not reachable from it, and the
checkout is reachable from no run but that delegate and the delegates it starts.

Where that cannot hold, a checkout is refused and no delegate starts: where a directory the parent
opened ([TRUST-9](trust-map.md#TRUST-9)) holds the working directory or the checkout, or where the
working directory holds the checkout, as it does for a session in the home directory. The
refusal the planner is given names that directory and says the person can close it with `/clear`,
which starts a new conversation, or start bravebot again without it.

A read refused for being outside the workspace says, where opening the directory it is in would
open one that holds the working directory, that no delegate is given a checkout while that
directory is open. It names the drop first, which reaches the file and costs nothing. A directory
that holds nothing of the kind is offered without the warning.

What follows from the root follows unchanged. A command line starts at the checkout's root
([CMDLINE-12](tools/command-line.md#CMDLINE-12)), a hook runs there
([HOOK-4](hooks.md#HOOK-4)), and a relative path means a path in the checkout.

The delegate is told, in the driver's words, which commit the checkout holds, that changes the
person has not committed are not in it, which paths a deny rule left out, that a `git fetch` in
it updates the refs of the working directory and of every other checkout, and that it shares one
stash with them, so it is not to use `git stash` ([CHECKOUT-5](#CHECKOUT-5)).

The planner is told those last two parts too, in the description of `isolation` and in the answer
to a spawn that made one checkout or several. Where its list holds `run`, it is told to fetch once
itself before starting the delegates that need a fetch, and where it does not, to ask one of them
for it. With or without `run`, it is told not to ask a delegate in a checkout to use `git stash`.

**Why reach without a person opening it.** On grounds like those the session's own directory is
reached on ([TRUST-16](trust-map.md#TRUST-16)): it was created here and is owned by this account.
That directory is also empty, and a checkout is not. What stands in for that is that a checkout
holds only what the driver wrote into it from a `.git` the map trusts in full, and each file in it
carries the label the same path has in the working directory ([CHECKOUT-8](#CHECKOUT-8)), so the
reach brings no file into the session that nobody has an answer about.

Built. The overlap refusals are made before anything is created. The delegate is told the commit,
that changes not committed are not in it, the paths left out, and that its refs and its stash are
shared. The answer to the spawn names the commit and says the refs and the stash are shared, and so
does the description of `isolation`.

`verified-by: bravebot_agent::turn::a_delegate_given_a_checkout_writes_there_and_not_in_the_working_directory`
`verified-by: bravebot_agent::workspace::each_checkout_is_a_numbered_workspace_under_the_state_directory`
`verified-by: bravebot_agent::workspace::a_checkout_is_refused_where_it_would_overlap_a_tree_the_session_opened`
`verified-by: bravebot_agent::workspace::a_checkout_refusal_names_the_added_directory_that_holds_the_working_directory`
`verified-by: bravebot_agent::workspace::a_checkout_refusal_names_the_added_directory_that_holds_the_checkouts`
`verified-by: bravebot_agent::workspace::a_read_refusal_for_a_directory_holding_the_workspace_says_what_opening_it_costs`
`verified-by: bravebot_agent::turn::the_answer_to_a_spawn_in_a_checkout_says_the_checkouts_share_the_repositorys_refs`
`verified-by: bravebot_agent::tools::the_isolation_field_says_checkouts_share_refs_and_who_fetches_once`

<a id="CHECKOUT-8"></a>
### CHECKOUT-8: a file in a checkout is labelled as the same path in the working directory is

As a checkout is made, each rule the map holds under the working directory is copied to the same
relative path under the checkout, and what was said about the workspace
([TRUST-7](trust-map.md#TRUST-7)) answers for every path no copied rule names. Re-spelling the map
for a new root, as `/cd` does ([TRUST-13](trust-map.md#TRUST-13)), is not that: every rule would
still name the file it named, and none would answer for a path in the checkout.

The copied rules join the session's live file authority as rules outside the project. A write in
the checkout is then recorded under the checkout's path ([TRUST-4](trust-map.md#TRUST-4),
[TRUST-5](trust-map.md#TRUST-5)), comes back to the session as any delegate's decision does
([DELEGATE-11](delegation.md#DELEGATE-11)), and is kept in the session record in full, as a rule
outside the project is ([TRUST-6](trust-map.md#TRUST-6)). A checkout's rules go when it is
removed, except those that distrust a path, which [CHECKOUT-12](#CHECKOUT-12) keeps.

`~/.bravebot` is read as trusted by provenance ([TRUST-11](trust-map.md#TRUST-11)). A checkout is
not: it holds a project's files, and is read through the map whatever directory it is in.

**Why the path's own rule.** `read_git show HEAD:<path>` is labelled by all of `.git` and by the
rule over `<path>` ([GIT-3](tools/read-git.md#GIT-3)), and when a checkout is made its file at
`<path>` holds exactly those bytes. With all of `.git` trusted ([CHECKOUT-4](#CHECKOUT-4)), the
meet of the two is the path's own rule.

Built. The copy is of the live authority's rules under the working directory, so a write in a
checkout is recorded under the checkout's path and comes back as any delegate's decision does.
Removing a checkout withdraws its rules except those that distrust a path.

`verified-by: bravebot_agent::workspace::a_checkout_is_labelled_as_the_working_directory_is`
`verified-by: bravebot_agent::workspace::a_checkout_is_removed_unless_something_was_done_in_it`
`verified-by: bravebot_core::trust::a_copied_subtree_answers_as_the_original_does`
`verified-by: bravebot_core::trust::a_copy_of_an_undecided_root_is_not_answered_by_a_broader_rule`
`verified-by: bravebot_core::trust::withdrawing_a_subtree_keeps_what_distrusts_a_path`
`verified-by: bravebot_core::file_authority::a_rooted_handle_files_a_relative_name_under_its_root`
`verified-by: bravebot_core::file_authority::copying_a_subtree_moves_no_revision`

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
working directory's, as they were resolved before the turn. What it is told about where it works
is the checkout.

A delegate in a checkout keeps no definition memory ([MEMORY-2](definition-memory.md#MEMORY-2)),
and the answer to the spawn says so.

**Why.** A checkout is the same tree at HEAD, and a rule written about the project is a rule about
it. A source read from it would be a second copy of the project's configuration, which a delegate
writing there could change for the delegates it starts.

**Why no memory.** The memory file is in the working directory, which the delegate does not
reach. The checkout's copy is HEAD's, and what the delegate wrote to it would stay in the checkout
until a person brought it back.

Built. A relative specifier is held against the checkout's root, since the delegate keeps the
relative gate names. An absolute specifier under the working directory is copied to the checkout
in the delegate's own copy of the rules, as another spelling of the same rule, so it is withdrawn
with the delegate. A rule whose written-out stretch stops short of the working directory, or holds
a wildcard before it, is not copied. Nothing in a checkout is read as a source, and a delegate in
one is given no definition memory. The answer to the spawn says so.

`verified-by: bravebot_agent::turn::a_delegate_in_a_checkout_reads_the_working_directorys_instructions`
`verified-by: bravebot_agent::turn::a_delegate_given_a_checkout_writes_there_and_not_in_the_working_directory`
`verified-by: bravebot_agent::turn::a_deny_rule_with_an_absolute_specifier_holds_in_a_delegates_checkout`
`verified-by: bravebot_core::permissions::a_rule_under_the_working_directory_is_copied_to_the_checkout`
`verified-by: bravebot_core::permissions::a_rule_outside_the_working_directory_is_not_copied_to_the_checkout`

<a id="CHECKOUT-10"></a>
### CHECKOUT-10: a command vouched for in the working directory is asked about again in a checkout

An entry a person vouched for this session names the tree it was given in
([RUN-8](tools/run.md#RUN-8)), and a checkout is not that tree. A line remembered past the session
([RUN-19](tools/run.md#RUN-19)) names no tree and is spelled against the workspace root, which in a
checkout would be the checkout's, so in a checkout it is not honoured. A run in a checkout is asked
about unless an entry made this session names the checkout.

**Why.** `sh check.sh` vouched for at the root is a statement about the file the root holds, and
the checkout's file at that path may differ.

Built. A delegate in a checkout is given no remembered lines, and an entry made this session names
the tree it was given in.

`verified-by: bravebot_agent::turn::a_command_vouched_for_in_the_working_directory_is_asked_about_again_in_a_checkout`

<a id="CHECKOUT-11"></a>
### CHECKOUT-11: a write in a checkout asks what the same write in the working directory asks

A path in a checkout takes every gate the same path in the working directory takes, and asks the
same question. The session's own directory is held to the same rule
([TRUST-16](trust-map.md#TRUST-16)): a place to write is not a place writes stop being asked about.

Bringing the file back asks again ([CHECKOUT-14](#CHECKOUT-14)). Whether the first question can go
is an open question below.

Built. A path in a checkout carries the rule the same path has in the working directory
([CHECKOUT-8](#CHECKOUT-8)), so it takes the gates that path takes.

`verified-by: bravebot_agent::turn::a_write_in_a_checkout_asks_what_the_same_write_in_the_working_directory_asks`

<a id="CHECKOUT-12"></a>
### CHECKOUT-12: `read_git` in a checkout is routed by the driver's record, and history anywhere in the session meets the checkouts' rules

A question about the checkout's repository is answered from the common directory and the
`worktrees/<id>/` entry the driver recorded when it made the checkout. `<checkout>/.git` is never
read. The files a read opens are the entry's HEAD and index beside the common directory's own, and
[GIT-2](tools/read-git.md#GIT-2)'s rules are held against that list. A status
([GIT-11](tools/read-git.md#GIT-11)) is then answered in the checkout on its usual terms.

An answer about the repository's history, in the working directory or in any checkout, is labelled
by the rule over each path it showed there and by the rule over the same path in every checkout of
that repository the session has made ([GIT-3](tools/read-git.md#GIT-3)). A rule that distrusts a
path in a checkout is kept for this after the checkout is removed, for as long as the session
lasts.

**Why not the file.** A delegate can overwrite it, and what it names would then choose which
repository is read.

**Why the checkouts' rules.** The checkouts and the working directory share one repository, so a
commit or a branch a program made in a checkout is history the working directory reads. Labelled by
the working directory's rule alone, a file a write left untrusted in the checkout would reach the
planner as trusted through that history. The meet is taken over the paths
[GIT-3](tools/read-git.md#GIT-3) already labels an answer by, so it adds no decision of its own.

Built. `read_git` in a checkout opens the common directory and the entry the driver recorded under
`worktrees/<id>/`, whose `HEAD` and index it reads beside the common directory's own, and never the
checkout's `.git`. Every file it opens is held against the rules, the entry's among them. An answer
about history is labelled by the rule over each path in the checkouts as well as in the working
directory, and a rule that distrusts a path outlives its checkout.

`verified-by: bravebot_agent::workspace::read_git_in_a_checkout_is_answered_without_reading_its_dot_git`
`verified-by: bravebot_agent::workspace::a_checkout_reads_the_entrys_head_and_index_not_the_common_directorys`
`verified-by: bravebot_agent::workspace::a_rule_over_a_file_the_entry_holds_declines_a_read_in_a_checkout`
`verified-by: bravebot_agent::workspace::a_distrusted_path_in_a_checkout_labels_history_that_shows_it`

## Bringing work back

<a id="CHECKOUT-13"></a>
### CHECKOUT-13: the paths that can come back are the ones the driver recorded, and the ones a trusted status lists

A checkout's candidate paths are, first, every path the driver recorded a file effect on in it: a
`write_file`, an `edit_file`, and a redirection a command line wrote through
([CMDLINE-5](tools/command-line.md#CMDLINE-5)). A name the planner typed was chosen by a planner
holding nothing untrusted, so it is routing. A name a `path_ref` gave came out of a directory
nobody vouched for, and no planner is shown it ([WRITE-4](tools/write-file.md#WRITE-4)), so a
write through one is counted and its path never named. Second, where a status over the checkout would be answered
([GIT-11](tools/read-git.md#GIT-11)), the paths it lists. Where it would not, a file a program
wrote is not found, and wherever the candidates are given they say the status could not be read.

No path is a candidate for differing from HEAD on a comparison of bytes the driver made. A path a
deny rule covers is never one.

Half built. The driver records the name the planner typed for each file a `write_file` or an
`edit_file` wrote in the checkout. It records a redirection's name once the line has ended, where
the line left a file there and the credential scan did not take it back out. A name is placed by
its spelling, with `.` and `..` resolved and nothing on disk read, so a link is recorded by its own
name and not by its target's. A write through a reference is counted. A name outside the checkout,
in scratch or an added directory, is not recorded. A path a deny rule covered when it was written is
never recorded, since that write was refused. One a rule added later covers stays recorded, and
that rule would refuse bringing it back ([CHECKOUT-14](#CHECKOUT-14)). No status is read in a
checkout, because the driver does not yet ask for one, so the candidates always say it could not be
read.

`verified-by: bravebot_agent::workspace::a_checkout_records_the_paths_written_in_it`
`verified-by: bravebot_agent::turn::a_kept_checkout_is_named_with_the_paths_written_in_it`

<a id="CHECKOUT-14"></a>
### CHECKOUT-14: a file comes back as a write through the gate, one path at a time

`apply_checkout` takes two routing fields: `checkout`, the number of a checkout the session keeps
([CHECKOUT-5](#CHECKOUT-5)), and an optional `paths`, a subset of that checkout's candidates.
Without `paths` it takes every candidate. A number the session keeps no checkout for, or a path that
is not a candidate, is refused.

Each path is a write of the checkout file's bytes to the same path in the working directory,
through the gate a `write_file` with `contents_ref` takes ([tools/write-file.md](tools/write-file.md)).
Each is put to the person, even where [TRUST-4](trust-map.md#TRUST-4)'s table would ask nothing, a
yes is bound to that path alone ([WRITE-3](tools/write-file.md#WRITE-3)), and the map, the
permission rules and a rewind treat it as any write. The bytes keep the label the checkout's path
gives them, so a file a write left untrusted there is still untrusted once it has come back, and the
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

Half built. `apply_checkout` takes `checkout`, the number the report gave a kept checkout, and
`paths`, which are checked against the candidates the driver recorded by name before anything is
written. A path that is not one, or a number the session keeps nothing for, refuses the whole call.
Without `paths` it takes every named candidate. Each is read from the checkout only as a plain file
with no link followed anywhere between the checkout's root and the file, labelled as the map labels
the checkout's path ([CHECKOUT-8](#CHECKOUT-8)), which is the working directory's rule unless a
write in the checkout has distrusted it since, and written to the same path in the working
directory through the gate a `write_file` takes: the credential scan, the permission rules and the
single question per path. The question is asked wherever the trust map's table would ask nothing,
and the person is shown the difference from their file as it is now. The trail records an apply
that brought a file back ([CHECKOUT-19](#CHECKOUT-19)), and a delegate is not offered the tool.

The driver records the names the session wrote in the working directory, by the spelling the
planner typed, in the order the writes were made, and a checkout remembers how many there had been
when it was made. The question says so where a later write names the path. That covers a
`write_file`, an `edit_file`, a redirection and an apply, from the planner or from a delegate that
shares the working directory. A write through a reference is not named, and a program's own write
is not seen, so the note is absent for those and says nothing about the bytes.

Not built: a path a status lists as removed (no status is read in a checkout,
[CHECKOUT-13](#CHECKOUT-13)), a file written through a reference, and `/checkouts apply`. A file
over 16 MiB, or that is not text, is named and left.

`verified-by: bravebot_agent::turn::the_question_says_the_working_directory_was_written_since_the_checkout`
`verified-by: bravebot_tui::confirm::a_write_since_the_checkout_is_said_in_the_question`

`verified-by: bravebot_agent::turn::a_kept_checkouts_file_comes_back_through_a_question_the_table_would_not_ask`
`verified-by: bravebot_agent::turn::declining_the_question_brings_nothing_back`
`verified-by: bravebot_agent::turn::only_a_recorded_path_of_a_kept_checkout_is_brought_back`
`verified-by: bravebot_agent::workspace::a_checkouts_candidate_is_read_with_its_paths_label_and_nothing_else_is_read`

## How long one lasts

<a id="CHECKOUT-15"></a>
### CHECKOUT-15: a checkout nothing was done in goes with its delegate, and any other is kept until a person removes it or the session that keeps nothing ends

Whether a checkout is kept is decided from the driver's record, never by comparing its files with
HEAD. Where neither the delegate given it nor any delegate that one started recorded a file effect
there or started a program there, a hook among them, the checkout is removed as the delegate given
it ends, with its `worktrees/<id>/` entry and its rules, save those
[CHECKOUT-12](#CHECKOUT-12) keeps, and nobody is asked: the record says it holds only what the
driver wrote.

Any other checkout is kept, in a session that keeps no record only until the session ends
([CHECKOUT-6](#CHECKOUT-6)). The session record holds its path, its commit, its number, the number
of the delegate given it, and the paths the driver recorded a file effect on in it
([SESSION-3](sessions.md#SESSION-3)), so a resume has the candidates
[CHECKOUT-13](#CHECKOUT-13) names. Leaving the session names each one kept
([SESSION-8](sessions.md#SESSION-8)).

`/checkouts` lists the checkouts the session keeps, shows one's candidates against the working
directory, and removes one. Removing one that has candidates asks first. The list says what each
one took on disk as its delegate ended, and whether a remote branch is at the commit its HEAD is
at, so a person can tell which ones are large and which are at a commit already pushed.

**Why the front end reads the branches.** A program in the checkout can write its HEAD and the
repository's refs, so what they say is untrusted. It is shown to the person and decides nothing:
the driver reads none of it, and the planner is told none of it.

Half built. A checkout the delegate given it and the delegates that one started did nothing in is
removed as the delegate ends, with its `worktrees/<id>/` entry and its rules. Any other is kept,
the planner is told where, and `/status` lists it ([CHECKOUT-21](#CHECKOUT-21)).

`/checkouts` lists each one kept with the paths the planner typed for its writes and the number of
writes made through a reference ([CHECKOUT-13](#CHECKOUT-13)), and says its status was not read.
It compares nothing with the working directory.

It gives each one's size, measured once as its delegate ends: the blocks the file system gives
everything beneath it and beneath its `worktrees/<id>/` entry, with no link followed and, on Unix,
a file with several names counted once. On Windows each name counts at the file's length. A link
in place of either directory is not measured. Measuring stops after two seconds, and the parent
waits for it; a size it did not finish is given as at least what it counted. A checkout whose
delegate is still running is said to be measured when it ends. The size is spelled in the units
of the person's line ([CHECKOUT-18](#CHECKOUT-18)).

It reads the HEAD in the checkout's `worktrees/<id>/` entry, never `<checkout>/.git`
([CHECKOUT-12](#CHECKOUT-12)), and the repository's loose and packed refs, once for each
repository listed. A file is opened without following a link in its place, each directory on the
way to it is checked not to be a link first, and only a plain file is read, opened so that a pipe
does not hold the listing. A name git would not take as a ref is not read, so one cannot add a
line to the listing. It names the branch HEAD
is on, or that it is on none, and a remote branch at the same commit, preferring the one of the
same name. A remote branch at a later commit is not looked for, so a checkout whose commit was
pushed and then built on elsewhere reads as at a commit no remote branch is at. A HEAD it cannot
follow, and remote branches it cannot read in full, are said to be unread.

`/checkouts remove <n>` removes one the way a
delegate's ending does, from the repository it was made from, wherever `/cd` has moved the session
since. It asks first where the record shows anything done there, a program started there included,
since the status that would name what a program wrote is not read. One that could not be removed as
its delegate ended is removed without a question. One holding the working directory or a directory
added by name is kept, and the person is told why: every read, write and run there would fail once
it went.

No session record holds a checkout, and leaving the session names none, so one nobody removes
stays until a person deletes it and runs `git worktree prune`.

`verified-by: bravebot_agent::turn::a_checkout_nothing_was_done_in_is_removed_when_its_delegate_ends`
`verified-by: bravebot_agent::turn::a_checkout_is_kept_when_a_line_in_it_was_refused_for_a_credential`
`verified-by: bravebot_agent::turn::a_checkout_is_kept_when_a_line_in_it_failed_after_a_stage_started`
`verified-by: bravebot_agent::workspace::a_checkout_is_removed_unless_something_was_done_in_it`
`verified-by: bravebot_agent::workspace::a_kept_checkout_is_removed_by_its_number`
`verified-by: bravebot_agent::git::removing_a_checkout_takes_its_directory_and_its_entry_alone`
`verified-by: bravebot_agent::git::removing_a_checkout_leaves_everything_where_worktrees_is_a_link`
`verified-by: bravebot_tui::state::the_checkouts_report_names_what_was_done_in_each`
`verified-by: bravebot_tui::state::the_checkouts_report_says_what_each_takes_and_whether_it_is_pushed`
`verified-by: bravebot_agent::git::a_checkout_is_measured_with_each_file_once_and_no_link_followed`
`verified-by: bravebot_agent::git::a_size_measuring_did_not_finish_is_a_lower_bound`
`verified-by: bravebot_agent::git::two_sizes_together_are_whole_only_where_both_are`
`verified-by: bravebot_agent::workspace::the_session_lists_each_checkout_it_has_until_one_is_removed`
`verified-by: bravebot_tui::checkouts_command::a_checkout_reads_as_pushed_where_a_remote_branch_is_at_its_commit`
`verified-by: bravebot_tui::checkouts_command::the_remote_branch_named_is_the_one_of_the_same_name`
`verified-by: bravebot_tui::checkouts_command::a_head_that_cannot_be_followed_reads_as_unread`
`verified-by: bravebot_tui::checkouts_command::a_link_is_not_followed`
`verified-by: bravebot_tui::checkouts_command::a_name_git_would_not_take_is_not_read`
`verified-by: bravebot_tui::checkouts_command::a_packed_ref_that_is_not_utf8_leaves_the_others_read`
`verified-by: bravebot_tui::checkouts_command::a_ref_that_cannot_be_read_reads_as_unread`
`verified-by: bravebot_tui::checkouts_command::a_pipe_is_not_waited_on`
`verified-by: bravebot_tui::checkouts_command::a_branch_git_pushed_reads_as_pushed`
`verified-by: bravebot_tui::app::a_checkout_worked_in_is_removed_only_when_the_person_says_so`
`verified-by: bravebot_tui::app::a_checkout_nothing_was_done_in_is_removed_without_asking`
`verified-by: bravebot_tui::app::a_checkout_not_kept_or_not_removable_is_said_so`
`verified-by: bravebot_tui::confirm::the_remove_checkout_question_names_the_checkout_and_what_removing_it_deletes`
`verified-by: bravebot_tui::confirm::a_remove_checkout_question_takes_a_yes_only_from_a_draw_showing_what_it_removes`

<a id="CHECKOUT-16"></a>
### CHECKOUT-16: a resume brings kept checkouts back, a fork does not, and an opening session removes what no session lists

`--resume` brings back the checkouts the record kept, with their rules. A fork carries neither the
checkouts nor the rules copied for them ([SESSION-18](sessions.md#SESSION-18)), so no two records
list one directory. `/cd` is refused
while the session keeps a checkout, and names it, since the record that lists it would move with
the session ([SESSION-13](sessions.md#SESSION-13)) and the checkout would stay keyed under the
directory it left.

On Unix a session takes a lock on a checkout's directory as it creates it, before anything is
written there, and holds it for as long as the session runs. Opening a session removes a directory
under its workspace's key in `checkouts/` that no session record for that workspace lists and no
running session holds, with its `worktrees/<id>/` entry. As [TRUST-25](trust-map.md#TRUST-25)'s
sweep does, it takes only what is this account's, and judges a link there as a link rather than as
what it points at. On Windows nothing is removed, and `/checkouts` names a checkout no record lists
and leaves it to the person.

**Why a lock as well as the record.** A session opening beside one that is running would otherwise
take a checkout that session is still making, or made since it last wrote its record, for a
leftover.

Partly built. `/cd` is refused while the session lists a checkout, and the refusal names each one
by its number and says to remove it with `/checkouts remove`. The resume, the fork rule, the lock
and the opening sweep are not built: no session record holds a checkout
([CHECKOUT-15](#CHECKOUT-15)), `scratch.rs`'s lock covers the session's own directory alone, and
nothing removes an unlisted directory under `checkouts/`.

`verified-by: bravebot_agent::workspace::a_move_is_refused_while_the_session_keeps_a_checkout_and_names_it`

<a id="CHECKOUT-17"></a>
### CHECKOUT-17: a rewind leaves a checkout as it is, and says so

A rewind ([SESSION-19](sessions.md#SESSION-19)) puts back nothing in a checkout and records the
gap in its coverage, as it does for the session's own directory
([TRUST-19](trust-map.md#TRUST-19)). A file brought back from one is a write in the working
directory and is rewound as any write is. A delegate still ends with the turn that started it
([DELEGATE-17](delegation.md#DELEGATE-17)), so nothing runs in a kept checkout once its turn is
over.

Built for the rewind. A delegate's checkout has backups of its own, which a rewind never reads, so it
puts back nothing there. A write in a checkout, and a program run in one, mark the session's
coverage with a `checkout` gap, which `/undo` names as delegate checkouts beside the other causes.
A file brought back from a checkout is a write in the working directory
([CHECKOUT-14](#CHECKOUT-14)), which is not built yet.

`verified-by: bravebot_agent::workspace::a_write_in_a_checkout_is_a_gap_in_the_sessions_rewind_coverage`
`verified-by: bravebot_agent::workspace::a_command_gap_in_a_checkout_is_also_a_checkout_gap_in_the_sessions_coverage`

## What else changes

<a id="CHECKOUT-18"></a>
### CHECKOUT-18: the driver names the checkout beside the report

Beside a delegate's report ([DELEGATE-8](delegation.md#DELEGATE-8)), the driver says in its own
words where the delegate's checkout is, the commit it holds, and its candidate paths, or that it was
removed. It is one more item the driver writes beside the report
([DELEGATE-9](delegation.md#DELEGATE-9)), and all of it is the driver's record.

Built. Beside the report the driver says where a kept checkout is and the commit it holds, the
same for one that could not be removed, or that it was removed. For a kept one it names each
candidate path whose name the planner typed, the first twenty and then how many more, counts the
writes made through a reference, and says the status could not be read
([CHECKOUT-13](#CHECKOUT-13)). It then says `apply_checkout` brings the files back ([CHECKOUT-14](#CHECKOUT-14)).

The person's line for the delegate's ending names a kept checkout by its number and gives what it
took on disk ([CHECKOUT-15](#CHECKOUT-15)), in kilobytes under a megabyte, megabytes under a
gigabyte and gigabytes from there, the unit chosen after rounding so that a size just short of a
gigabyte reads as 1.0 GB. The planner is not told the size: it measures files programs
in the checkout wrote, and nothing the planner does turns on it.

`verified-by: bravebot_agent::turn::a_delegate_given_a_checkout_writes_there_and_not_in_the_working_directory`
`verified-by: bravebot_agent::turn::the_person_is_told_what_a_kept_checkout_takes_on_disk_and_the_planner_is_not`
`verified-by: bravebot_agent::git::a_size_is_spelled_in_kilobytes_megabytes_or_gigabytes`
`verified-by: bravebot_agent::turn::a_checkout_nothing_was_done_in_is_removed_when_its_delegate_ends`
`verified-by: bravebot_agent::turn::a_kept_checkout_is_named_with_the_paths_written_in_it`
`verified-by: bravebot_agent::delegate::a_kept_checkouts_note_names_twenty_paths_and_counts_the_rest`

<a id="CHECKOUT-19"></a>
### CHECKOUT-19: the trail records each checkout made, applied from and removed

Each is an event with the checkout's path, and one a delegate's run took keeps that run's number
([TRACE-4](trace.md#TRACE-4)). The commit and the checkout's number are not in it, since the
trail's fields are gate names, capabilities, labels, paths, hosts and slot ids
([TRACE-2](trace.md#TRACE-2)). The session record holds both against the path.

Built for the making and the removal, whether the delegate's ending or `/checkouts remove` removed
it. A checkout a delegate's own run made for a delegate of its own carries that run's number, and
one the turn made carries none. Applying from a checkout is recorded as `applied from <path>`,
once for each `apply_checkout` call that brought a file back, and not for one that brought none
([CHECKOUT-14](#CHECKOUT-14)).

`verified-by: bravebot_agent::turn::the_trail_records_a_checkout_made_and_removed_with_its_path`
`verified-by: bravebot_agent::turn::the_trail_records_no_removal_for_a_checkout_that_was_kept_and_holds_neither_commit_nor_number`
`verified-by: bravebot_agent::turn::a_checkout_a_delegate_made_is_recorded_under_that_delegates_number`
`verified-by: bravebot_tui::app::removing_a_checkout_is_recorded_with_its_path_and_a_kept_one_is_not`
`verified-by: bravebot_agent::turn::a_kept_checkouts_file_comes_back_through_a_question_the_table_would_not_ask`
`verified-by: bravebot_agent::turn::declining_the_question_brings_nothing_back`

<a id="CHECKOUT-20"></a>
### CHECKOUT-20: a delegate in a checkout is offered no `lsp`

The session's language servers are rooted at the working directory and shared with its delegates
([LSP-9](tools/lsp.md#LSP-9)), so a path in a checkout is outside the workspace for them
([LSP-4](tools/lsp.md#LSP-4)). A delegate in a checkout is not offered `lsp`, and the answer to the
spawn says so.

**Why not a server per checkout.** It costs a second index and a second approval
([LSP-5](tools/lsp.md#LSP-5)), and is left for a later change.

Built. A delegate in a checkout is given no language servers, is not offered the `lsp` tool
whatever its kind holds, and the answer to the spawn says so.

`verified-by: bravebot_agent::turn::a_delegate_given_a_checkout_writes_there_and_not_in_the_working_directory`
`verified-by: bravebot_agent::turn::a_delegate_in_a_checkout_is_offered_no_lsp`

<a id="CHECKOUT-21"></a>
### CHECKOUT-21: `/status` lists the session's checkouts

Each checkout the session made and has not removed is a line of its own, with its number, its
path, its commit and the number of the delegate given it, as the session's own directory has one
([TRUST-14](trust-map.md#TRUST-14)). A checkout carries rules, so it is in the rules in force as
well ([TRUST-12](trust-map.md#TRUST-12)).

Built. The lines follow the session's own directory, oldest checkout first, and show the first ten
characters of the commit, as a log line does. One that could not be removed is listed, since it is
still on disk, and one whose directory is gone, removed by hand or only half removed, is not. The
rules copied for a checkout are in the trust map the turn hands back, which is the map `/status`
lists. A delegate's number is its turn's, so two checkouts made in two turns can both name `d1`.
The checkout's own number is the session's and tells them apart. The list is held in memory: a
session `/clear` begins lists none of the checkouts made before it, and a resumed one lists none
it made before it was left, until [CHECKOUT-16](#CHECKOUT-16) is built.

`verified-by: bravebot_agent::workspace::the_session_lists_each_checkout_it_has_until_one_is_removed`
`verified-by: bravebot_agent::workspace::a_session_begun_over_lists_none_of_the_checkouts_made_before_it`
`verified-by: bravebot_agent::turn::the_session_lists_the_checkout_a_delegate_kept_and_the_rule_copied_for_it`
`verified-by: bravebot_tui::status::each_checkout_the_session_has_is_a_line_of_its_own`
`verified-by: bravebot_tui::status::a_session_with_no_checkout_reports_none`

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
- **The driver runs one `git fetch` at a time across checkouts.** A fetch is one of the commands a
  program in a checkout can run, and `git pull`, `git remote update` or a build script move the
  same refs. A lock on the command's name would cover one way of spelling it, and the working
  directory's refs would still move ([CHECKOUT-5](#CHECKOUT-5)).
- **`run` refuses `git stash` in a checkout.** `git rebase --autostash`, `git pull --autostash`, an
  alias or a script stash as well, so a refusal on the command's name would cover one way of
  spelling it ([CHECKOUT-5](#CHECKOUT-5)).

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
- **Checkouts share the repository's refs.** A `git fetch` a delegate runs in one updates the
  remote-tracking refs of the working directory and of every other checkout, and fetches in several
  at once fail ([CHECKOUT-5](#CHECKOUT-5)). The planner is told to have the fetch done once
  ([CHECKOUT-7](#CHECKOUT-7)), and nothing stops a delegate fetching anyway.
- **Checkouts share one stash.** A `git stash pop` a delegate runs in one can apply an entry the
  person or another checkout stashed, and drop it ([CHECKOUT-5](#CHECKOUT-5)). Each delegate is
  told not to use `git stash` ([CHECKOUT-7](#CHECKOUT-7)), and nothing stops one stashing anyway.
- **A repository with a filter or an end-of-line conversion gets no checkout**, and that includes
  every repository using LFS.
- **A worker's first build is asked about.** A command vouched for in the working directory is not
  vouched for in a checkout ([CHECKOUT-10](#CHECKOUT-10)).
- **A write is asked about twice**, once in the checkout and once as it comes back.
- **A global or system `core.autocrlf` on Unix goes unseen.** It is not read
  ([CHECKOUT-4](#CHECKOUT-4)), so where one is set the checkout holds a file's bytes as stored where
  the person's own git would have written them converted.
- **No definition memory in a checkout** ([CHECKOUT-9](#CHECKOUT-9)). A definition that asks for a
  checkout forgets between conversations.
- **A session whose added directory holds the working directory gets no checkout**, nor one in the
  home directory ([CHECKOUT-7](#CHECKOUT-7)).
- **A killed session that kept nothing leaves `worktrees/<id>/` entries** in the person's `.git`
  ([CHECKOUT-6](#CHECKOUT-6)). `git worktree prune` removes them.
- **A file a program wrote is not found where a status is not answered**, and no status is answered
  in a checkout yet ([CHECKOUT-12](#CHECKOUT-12)). The driver's record holds only what the file
  tools and redirections wrote ([CHECKOUT-13](#CHECKOUT-13)).
- **A write through a reference is counted, not named**, so the planner learns that a file it
  cannot name was written and not which one ([CHECKOUT-13](#CHECKOUT-13)).
- **No `lsp` in a checkout** ([CHECKOUT-20](#CHECKOUT-20)).
- **Nothing brings a checkout's work back yet** ([CHECKOUT-14](#CHECKOUT-14)), and a kept checkout
  stays until a person removes it with `/checkouts remove` ([CHECKOUT-15](#CHECKOUT-15)).
  `/status` and `/checkouts` say where it is ([CHECKOUT-21](#CHECKOUT-21)).
- **In a session that keeps nothing, a checkout goes with the session**, with whatever was not
  brought back.
- **A checker that ran a program keeps its checkout**, since it started a program there
  ([CHECKOUT-15](#CHECKOUT-15)), and a person removes it.
- **On Windows a killed session's checkouts stay** until a person removes them
  ([CHECKOUT-16](#CHECKOUT-16)).

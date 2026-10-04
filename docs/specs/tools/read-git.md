---
id: GIT
title: read_git
status: normative
governs:
  - crates/agent/src/git.rs
  - crates/agent/src/git/status.rs
  - crates/agent/src/workspace.rs
  - crates/agent/src/tools.rs
  - crates/core/src/policy.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Reading a repository's history from the files under its `.git`, and its status from those and the
working tree, without starting git. `query`,
`repository`, `revision`, `path`, `pattern`, `since`, `until`, `count`, `skip` and `messages` are
routing: the first names which question is asked, the next four name which repository, which
commits, which files and which lines it is asked about, and the rest bound what a log, a list of
tags or a search lists and how much of each it prints.
The only content argument is the `why` every
tool takes ([TOOL-5](tool-surface.md#TOOL-5)). The result is
the answer, or a reference where a path it showed is untrusted. What it shows is scanned for
credentials before the planner receives it, as a file read is
([credential-protection.md](../credential-protection.md)): the lines it prints of a file are
scanned as a read of that file, one side of a diff at a time and from the line they start at, and
the rest of the answer as a read of `.git`. A person agreeing to be shown one file's key has agreed
to nothing about another's.

## Clauses

<a id="GIT-1"></a>
### GIT-1: history is read from the files under `.git`, and no program is started

`log` lists commits one per line, newest first: the first ten characters of the id, the day it was
authored, its author and its subject, as `git log --format='%h %ad %an %s' --date=short
--abbrev=10` prints them, and asked for messages, the rest of each message beneath its line
([GIT-12](#GIT-12)).
`show` prints a commit with its message and its diff as `git show` does, a tag with its message and
then its commit, and `<revision>:<path>` as the file or directory stood at that revision. `diff`
compares two commits' trees as `git diff` prints them. Revisions are read in git's syntax, as git
resolves them: names, ids and their prefixes, `~N`, `^N`, `^{commit}` and ranges. Loose and packed
objects, deltas included, read the same.

What each question means is fixed by this reader. No key in the repository's configuration names a
program it runs, a driver it applies or a filter it passes bytes through, and no remote URL is ever
part of an answer. A merge is shown without a diff and names the diff to ask for, a binary file is
described by its size, a submodule is shown as the commit it records, and a shallow commit is read
as a root, as git reads it.

**Why.** git runs what its configuration names: `core.fsmonitor` on a status, `gpg.program` on a
log that shows signatures, a pager and an alias on anything, and `include.path` pulls that
configuration in from any file. The configuration is part of the tree being inspected, so running
git over an unknown repository is running a program somebody else wrote, which is why
[command-line.md](command-line.md) keeps git on the road where a person vouches for it. A
reader whose behaviour no file can change answers the common question, what happened in this
history, with nobody asked.

Built on gitoxide's plumbing crates for hashes, objects, packs and refs, none of which starts a
process. The history walk, the tree diff and the line diff are written here, and `deny.toml` bans
the gix crates that start one.

`verified-by: bravebot_agent::turn::read_git_shows_the_planner_the_history_of_a_trusted_repository`
`verified-by: bravebot_agent::git::log_lists_commits_newest_first_one_line_each`
`verified-by: bravebot_agent::git::show_prints_a_commit_its_message_and_its_diff_as_git_does`
`verified-by: bravebot_agent::git::show_lists_a_directory_and_prints_a_file_at_a_revision`
`verified-by: bravebot_agent::git::diff_compares_two_commits_as_git_diff_prints_them`
`verified-by: bravebot_agent::git::revisions_resolve_through_names_ancestry_and_peels_as_git_does`
`verified-by: bravebot_agent::git::ranges_and_pairs_are_read_as_git_reads_them`
`verified-by: bravebot_agent::git::a_range_leaves_out_what_its_start_already_reaches`
`verified-by: bravebot_agent::git::a_range_leaves_out_its_start_whatever_the_commit_times`
`verified-by: bravebot_agent::git::a_path_after_a_revision_is_read_as_a_name_whatever_it_holds`
`verified-by: bravebot_agent::git::a_path_keeps_only_the_commits_that_changed_it`
`verified-by: bravebot_agent::git::a_log_narrowed_to_a_path_follows_the_side_a_merge_kept_it_from`
`verified-by: bravebot_agent::git::bytes_that_are_not_utf8_are_shown_escaped_and_diffed_as_bytes`
`verified-by: bravebot_agent::git::a_tag_is_shown_with_its_message_then_its_commit`
`verified-by: bravebot_agent::git::a_merge_is_shown_without_a_diff_and_names_the_diff_that_gives_one`
`verified-by: bravebot_agent::git::a_binary_file_is_described_rather_than_printed`
`verified-by: bravebot_agent::git::a_submodule_is_shown_as_the_commit_it_records_and_not_read`
`verified-by: bravebot_agent::git::a_shallow_commit_is_read_as_having_no_parents`
`verified-by: bravebot_agent::git::a_signature_time_is_shown_in_its_own_zone`
`verified-by: bravebot_agent::git::objects_in_a_pack_and_stored_as_deltas_read_as_loose_ones_do`

<a id="GIT-2"></a>
### GIT-2: a repository is opened only where the trust map trusts all of its `.git`

The map is asked about `.git` as a subtree, so a rule distrusting one file anywhere beneath it, a
pack somebody fetched or a ref somebody else wrote, keeps the repository closed. Nobody having said
anything about a repository is not trust in it. The question is asked of the name the planner wrote
and the rules alone, before any file under `.git` is read, and a repository it fails is refused
with a sentence pointing at `run`, where the turn is offered one.

The files a read opens are listed before any of them is decoded, and that list is what the rules
are held against. A file a read opens is on it; a file no read opens is not. That is the
configuration, whatever case its name is written in, the refs a name can reach, which leaves out a
`.lock` and any name with a part starting `.`, the loose objects, and each pack index with the pack
beside it. A status adds the index, `info/exclude` and `info/attributes`. A listing that runs past the search deadline is declined as a read out of time is.

**Why.** Reading history means following ids the files hold: a ref naming a commit, a commit its
parent and its tree, a tree its entries. Following them over bytes nobody vouched for is the driver
branching on untrusted content, which [labels.md](../labels.md) admits nowhere.

**Why this does not contradict [command-line.md](command-line.md).** There, `.git/config` is never
read to decide how a command is routed, because it is content nobody vouched for. Here nothing under
`.git` is read until the map vouches for all of it, and the configuration then decides only whether
this reader declines ([GIT-8](#GIT-8)), never what it runs.

`verified-by: bravebot_agent::turn::read_git_does_not_open_a_repository_nobody_vouched_for`
`verified-by: bravebot_core::policy::a_repository_is_trusted_beneath_only_where_nothing_inside_it_is_distrusted`
`verified-by: bravebot_core::policy::a_repository_nobody_vouched_for_is_not_trusted_beneath`
`verified-by: bravebot_agent::git::survey_lists_the_files_a_read_opens_and_none_it_does_not`
`verified-by: bravebot_agent::git::status_surveys_the_index_and_the_info_files_it_reads`

<a id="GIT-3"></a>
### GIT-3: an answer is labelled by the whole of `.git` and by every path it showed

A path it showed is the working-tree path of a file whose history the answer printed, spelled from
the workspace root, so a repository in `sub` shows `sub/<path>`. An answer showing a path the map
distrusts is as untrusted as that file, and reaches the planner as a reference, however trusted the
`.git` it came out of. A log naming no path showed none and is labelled by `.git` alone.

A path is held against the map only where it names one file one way. A path written through `..`,
`.` or a doubled slash is refused, a tree entry git would not check out, `..` or a name holding a
`/`, is not read, a name that is not UTF-8 is left out as [GIT-4](#GIT-4) leaves out a withheld
one, and a tree or file named by its id alone, with no path, is refused.

**Why.** A blob holds the bytes of the file it committed. Labelled by where it was stored rather
than by what it holds, a file the map distrusts would reach the planner through its history.

`verified-by: bravebot_agent::turn::a_history_answer_showing_a_distrusted_path_is_quarantined`
`verified-by: bravebot_agent::workspace::a_repository_below_the_root_is_read_under_the_rules_on_its_own_path`
`verified-by: bravebot_core::policy::a_repository_answer_is_untrusted_where_a_path_it_showed_is`
`verified-by: bravebot_core::policy::a_repository_answer_is_untrusted_where_anything_under_git_is`
`verified-by: bravebot_agent::git::a_path_not_plainly_inside_the_repository_is_refused`
`verified-by: bravebot_agent::git::a_tree_entry_whose_name_would_leave_its_directory_is_not_read`
`verified-by: bravebot_agent::git::a_path_whose_name_is_not_utf8_is_left_out_as_withheld`
`verified-by: bravebot_agent::git::a_tree_or_file_named_by_its_id_alone_is_refused`

<a id="GIT-4"></a>
### GIT-4: a deny rule over a file covers its history

A file the question names, by `path` or by `<revision>:<path>`, is refused under a deny rule as a
read of it is, before anything under `.git` is opened. A file a rule covers that an answer would
otherwise show, in a diff, a listing, a search or a log narrowed to it, is left out, and the answer
says something was left out.

A rule covering `.git` or any file beneath it keeps the repository closed. read_git reads every
file there or none.

Neither refusal says to read it with git instead, although git would print the bytes. A rule over a
path does not cover a command line, so the sentence would be the way round the rule that this clause
closes, and which paths a command line may reach is the permission rules' question rather than this
reader's.

**Why.** Otherwise `show HEAD:.env` is the way round every rule on `.env`. The repository closes
whole because a history with the denied file's objects removed is not one this reader can walk:
which object a file holds is not known until it is read.

`verified-by: bravebot_agent::git::a_refusal_a_deny_rule_caused_points_at_nothing_else`
`verified-by: bravebot_agent::turn::a_deny_rule_over_a_file_refuses_reading_its_history`
`verified-by: bravebot_agent::turn::a_repository_holding_a_file_a_deny_rule_covers_is_not_opened`
`verified-by: bravebot_agent::workspace::a_repository_a_deny_rule_names_is_not_opened`
`verified-by: bravebot_agent::workspace::a_rule_over_the_file_a_symlinked_repository_lands_on_leaves_it_out_of_a_commit`
`verified-by: bravebot_agent::workspace::a_rule_over_a_git_file_a_symlinked_repository_lands_on_keeps_it_closed`
`verified-by: bravebot_agent::git::a_withheld_path_is_left_out_of_every_answer_and_the_answer_says_so`
`verified-by: bravebot_agent::git::a_withheld_path_is_neither_read_nor_listed`

<a id="GIT-5"></a>
### GIT-5: the question is log, show, diff, status, tags or search, and anything else is refused by name

A word off the list is refused and the planner is told to use `run`, where the turn is offered one. A revision form this reader does not implement, `A...B` or `HEAD^@`
among them, is refused by name, and a query given the other shape of revision, one where it takes
two or two where it takes one, is told which query takes it. A status given a revision is told to
use diff. A pattern given to anything but search, a search given none, an empty one or one that is
not a regular expression, and a list of tags given a path are each refused by name.

**Why.** A guess answers a question the planner did not ask as though it had. `A...B` read as the
forms this reader knows is the range from `A.` to `.B`, and `HEAD^@` is `HEAD^`, so each would
come back as a confident answer to something else. A pattern or a path dropped without a word is
the same guess: a log with its pattern dropped is every commit, read as the ones that matched.

`verified-by: bravebot_agent::tools::a_query_off_the_list_names_run_only_where_the_turn_holds_one`
`verified-by: bravebot_agent::turn::read_git_answers_status_and_refuses_a_query_off_the_list`
`verified-by: bravebot_agent::turn::read_git_searches_the_files_at_a_revision_and_pages_the_lines`
`verified-by: bravebot_agent::turn::read_git_lists_the_tags_a_revision_reaches_newest_version_first`
`verified-by: bravebot_agent::git::revision_syntax_read_git_does_not_implement_is_refused_by_name`
`verified-by: bravebot_agent::git::a_query_given_the_wrong_shape_of_revision_says_which_query_takes_it`
`verified-by: bravebot_agent::git::a_pattern_is_for_search_alone_and_tags_take_no_path`

<a id="GIT-6"></a>
### GIT-6: `since` and `until` are whole days in UTC

Both ends are included: `until` reaches the last second of its day. A value that is not a day on
the calendar, written `YYYY-MM-DD`, is refused with the form it takes.

**Why.** A planner asking for commits since the 15th means the whole of the 15th, and a day like
`2023-02-30` read leniently lands on a neighbouring one with nobody told.

`verified-by: bravebot_agent::turn::since_and_until_are_whole_days_and_anything_else_is_refused`
`verified-by: bravebot_agent::git::since_and_until_bound_the_log_by_whole_days`
`verified-by: bravebot_agent::git::only_a_real_calendar_day_is_read_as_one`

<a id="GIT-7"></a>
### GIT-7: a git run whose output is sealed names read_git

Where a stage of a `run` was git and its output could not be shown to the planner, the result says
once that read_git reads history with nobody asked in a trusted repository. A run of any other
program says nothing about it.

**Why.** A planner that ran git and got a sealed result does not otherwise learn that the question
has an answer it can read. Said after every run, the sentence would be noise the planner learns to
skip.

`verified-by: bravebot_agent::turn::a_sealed_git_run_names_read_git_and_another_programs_does_not`

<a id="GIT-8"></a>
### GIT-8: a repository whose files change what a read means is declined, not followed

Declined, with a sentence pointing at `run` where the turn is offered one:

- a `.git` that is a file, or that holds a symbolic link where a read goes
- objects borrowed through `objects/info/alternates`, refs and objects shared through `commondir`,
  and objects replaced or parents grafted through `refs/replace` or `info/grafts`
- configuration that includes another file, sets `core.worktree`, or names a repository format or
  an extension this reader does not know, in the shared file or the per-worktree one
- a ref under `worktrees/` or `main-worktree/`, which lives in another worktree's directory
- symbolic refs that point at each other

Configuration git reads without changing what history means is read, whatever its comments,
quoting, line endings and continued lines.

[CHECKOUT-12](../checkouts.md#CHECKOUT-12) opens one kind of linked worktree: a delegate's
checkout, found from the driver's own record of it and never from its `.git` file.

**Why.** Each of these sends a read to files the list in [GIT-2](#GIT-2) does not hold, or answers
with objects other than the ones stored. Past any of them this reader answers a different question
from the one git would, and the rules were held against files it did not read.

`verified-by: bravebot_agent::git::a_repository_whose_files_send_a_read_elsewhere_is_declined`
`verified-by: bravebot_agent::git::a_symbolic_link_where_a_read_goes_is_declined_rather_than_followed`
`verified-by: bravebot_agent::git::configuration_that_changes_what_a_read_means_is_declined`
`verified-by: bravebot_agent::git::configuration_git_reads_to_the_same_meaning_is_accepted`
`verified-by: bravebot_agent::git::the_per_worktree_configuration_is_held_to_the_same_rules`
`verified-by: bravebot_agent::git::a_ref_in_another_worktrees_directory_is_never_read`
`verified-by: bravebot_agent::git::a_loop_of_symbolic_refs_is_declined`

<a id="GIT-9"></a>
### GIT-9: an answer is bounded, and one that stopped short says so

A log lists 20 commits, a list of tags 20 tags and a search 20 lines unless the planner names a
count, and never more than 200. An answer holds at most 2,000 lines and each line at most 2,000
characters. A file past 1 MiB is described by its size, read from the object's header alone, and
not searched. An answer stops at the search deadline. An answer cut
by any of these says it was cut, and one that was not makes no such claim.

**Why.** A repository's history is as large as the repository, and a planner handed part of it as
though it were the whole draws conclusions from what is missing.

`verified-by: bravebot_agent::git::a_log_that_reaches_its_count_stops_and_says_it_was_cut`
`verified-by: bravebot_agent::git::an_answer_past_its_lines_or_a_line_past_its_width_is_cut`
`verified-by: bravebot_agent::git::a_file_past_the_size_cap_is_described_by_its_size`
`verified-by: bravebot_agent::git::a_read_out_of_time_says_so`
`verified-by: bravebot_agent::git::a_listing_or_a_diff_that_fills_the_answer_says_it_was_cut_only_when_more_was_left`

<a id="GIT-10"></a>
### GIT-10: status lists what `git status --short --no-renames` lists

Staged changes compare the index with the tree at HEAD, unstaged ones the working tree with the
index, and untracked paths follow, sorted, each as `XY path` the way git prints them. A conflict is
coded by the stages the index holds (`UU`, `AA`, `DU` and the rest), a file added with intent is
` A`, or ` D` once its file is gone or a directory stands in its place, and a directory holding nothing tracked is one line, `dir/`, or
none where everything in it is ignored. A nested repository, a directory whose `.git` is a file or
holds a `HEAD`, reached through a symbolic link or not, is one line too. Only files and symbolic links are listed, never a fifo or a
socket, and `status.showUntrackedFiles` is read: `no` lists nothing untracked, and `all` lists each
file rather than its directory. A path filter lists that path and what is beneath it, not the
directories above it. A path holding a space, a quote, a backslash, a control character or, with
`core.quotePath` on as it is by default, a byte past ASCII is quoted as git quotes it. A tree where
nothing changed says so, and where the trust map withheld a path, says only that nothing changed
among the paths it could read.

A file whose stat data matches the index is taken as unchanged unless the index was written no
later than the file last changed, or the entry records a size of zero for content that is not
empty, as git takes it; any other file is read and hashed. A symbolic link is compared by where it
points, the executable bit counts where `core.filemode` says it does, a submodule replaced by a
file is a type change, and a path under a directory that is now a symbolic link is deleted rather
than followed. Ignore files are read as git reads them, per directory with `info/exclude` beneath
them, and so are attributes files. A file an attribute or `core.autocrlf` would convert on its way
into the index, and a submodule, is listed under a heading as not compared rather than guessed at.
An ignore or attributes file the trust map withholds is not guessed at either: nothing untracked
beneath a withheld ignore file is listed, though an untracked directory holding one is, as git lists
it for the ignore file itself, and a withheld attributes file leaves every file beneath its
directory whose stat data changed not compared. A rule withholding a name counts only where a file
stands at it, and a withheld ignore file counts only where untracked files are listed. A file
replaced between being looked at and being read is not compared either, and is never followed
through a link or waited on as a fifo. Renames are not detected. Nothing is written, the index included.

Declined, with a sentence pointing at `run` where the turn is offered one: a split or sparse index, one holding an extension
this reader does not know that git would need, one whose checksum does not match, one naming a
path git would not check out, a repository whose configuration sets `core.bare`, and one that
names an ignore or attributes file outside it through `core.excludesFile`, `core.attributesFile`
or `attr.tree`. The global and system configuration and the global ignore file are not read, so
on Windows, where Git for Windows sets `core.autocrlf` in the system file, line endings are taken
as converted unless the repository's own configuration says they are not.

**Why.** Status is the question a planner asks most, and the one git answers by running what
`core.fsmonitor` names. Where this reader cannot tell what git would print, it says so rather than
printing something else: a filter or line-ending conversion changes the bytes a file is hashed as,
and an index this reader does not parse in full lists entries it cannot see.

`verified-by: bravebot_agent::git::status_lists_staged_unstaged_and_untracked_as_git_status_short_does`
`verified-by: bravebot_agent::git::a_clean_tree_says_so_and_a_version_four_index_reads_the_same`
`verified-by: bravebot_agent::git::matching_stat_data_is_trusted_unless_the_index_is_as_new_as_the_file`
`verified-by: bravebot_agent::git::a_merge_conflict_is_coded_by_the_stages_the_index_holds`
`verified-by: bravebot_agent::git::a_file_an_attribute_converts_is_not_compared`
`verified-by: bravebot_agent::git::a_link_above_a_tracked_file_is_not_followed`
`verified-by: bravebot_agent::git::an_untracked_directory_is_one_line_and_one_of_only_ignored_files_is_none`
`verified-by: bravebot_agent::git::a_changed_executable_bit_is_a_change`
`verified-by: bravebot_agent::git::a_layout_status_cannot_read_as_git_would_is_declined`
`verified-by: bravebot_agent::git::a_path_with_a_space_is_quoted_as_git_quotes_it`
`verified-by: bravebot_agent::git::paths_are_quoted_and_filtered_as_git_quotes_and_filters_them`
`verified-by: bravebot_agent::git::an_intent_to_add_entry_whose_file_is_gone_is_a_deletion`
`verified-by: bravebot_agent::git::a_replaced_submodule_a_smudged_entry_and_a_socket_read_as_git_reads_them`
`verified-by: bravebot_agent::git::status_show_untracked_files_is_read_from_the_config`
`verified-by: bravebot_agent::git::a_withheld_attributes_or_ignore_file_is_not_guessed_at`
`verified-by: bravebot_agent::git::a_withheld_rule_counts_only_where_a_file_it_withholds_is_read`
`verified-by: bravebot_agent::turn::read_git_answers_status_and_refuses_a_query_off_the_list`

<a id="GIT-11"></a>
### GIT-11: status is answered only where the trust map trusts the whole working tree

The map is asked about the repository's directory as a subtree, as [GIT-2](#GIT-2) asks about
`.git`, and a tree it fails is refused with a sentence pointing at `run` where the turn is offered
one. log, show and diff in the
same repository are answered as before.

**Why.** Status reads every file in the tree: it hashes their bytes, reads their ignore and
attributes files, and prints their names. Each of those is the driver branching on the file, so a
file nobody vouched for keeps status closed, as a pack nobody vouched for keeps the repository
closed.

`verified-by: bravebot_agent::turn::status_is_answered_only_where_the_whole_working_tree_is_trusted`
`verified-by: bravebot_agent::workspace::a_status_below_the_root_is_asked_about_its_own_directory`

<a id="GIT-12"></a>
### GIT-12: a log is read a page at a time, with whole messages where asked

`skip` passes over that many of the commits a log matches before it lists any, as `git log --skip`
does. It counts the commits the log would list once its range, path and days are applied, not the
commits walked on the way to them. A log that stopped with commits still to list, at its count or at
its lines, names the skip that lists the next of them, and one that listed its last commit names
none.

With `messages`, each commit's line is followed by the rest of its message, every line indented
four spaces as `git show` indents it, so a line that is not indented starts a commit. A commit goes
on a page whole or waits for the next one. The exception is a commit whose message alone is longer
than an answer, which is shown as far as it fits and cut, so that the next skip moves past it.

**Why.** A commit's body is where it says why it was made and what it closes, and a history longer
than one answer is common. A planner given neither reruns git through `run`, where the answer may be
sealed, or narrows the question until commits drop out of it. A skip that counted walked commits
would pass over a different set from the one the planner was shown, and a message cut at the foot of
a page would be read as the whole of it.

`verified-by: bravebot_agent::turn::read_git_pages_through_a_log_and_shows_whole_messages_when_asked`
`verified-by: bravebot_agent::git::a_log_with_messages_prints_each_commit_whole_beneath_its_line`
`verified-by: bravebot_agent::git::skip_passes_over_the_commits_a_log_already_listed`
`verified-by: bravebot_agent::git::a_page_of_messages_ends_at_a_whole_commit`

<a id="GIT-13"></a>
### GIT-13: tags are listed newest version first, and with a revision only those it reaches

`tags` lists the repository's tags, loose and packed alike and a loose one over a packed one of the
same name, in the order `git tag --sort=-v:refname` lists them: a run of digits is compared as a
number, so `v0.10.0` comes before `v0.9.0`. Each line is the tag's name and the commit it names as a
log prints it, or for a tag that names a tree or a file, its id and what it is. Given a revision,
only the tags whose commit that revision's history holds are listed, as `git tag --merged` lists
them, and a tag naming no commit is not. A list of tags shows no path and is labelled by `.git`
alone. It pages by `count` and `skip` as a log does ([GIT-12](#GIT-12)), naming the skip that lists
the next of them.

**Why.** Where the last release ended is the newest tag the history being released holds. Sorted as
names, `v0.9.0` would be taken as newer than `v0.10.0`, and without a revision a tag on another
branch would be taken as the one before this release.

`verified-by: bravebot_agent::turn::read_git_lists_the_tags_a_revision_reaches_newest_version_first`
`verified-by: bravebot_agent::git::tags_are_listed_newest_version_first_loose_and_packed_alike`
`verified-by: bravebot_agent::git::tags_given_a_revision_are_only_those_its_history_holds`
`verified-by: bravebot_agent::git::tags_page_by_count_and_skip`

<a id="GIT-14"></a>
### GIT-14: a search is labelled by every file it read, matched or not

`search` prints the lines of the files at one revision, HEAD unless another is named, that
`pattern` matches, as `git grep -n` prints them: `path:line: text`, in the order git lists the tree,
with a binary file that matches named rather than printed and a file past the size cap described by
its size. A path narrows it to that file or the files beneath it. A symbolic link and a submodule
are not searched, as `git grep` over a tree searches neither. The pattern is written as
`file_grep`'s is ([SEARCH-1](search.md#SEARCH-1)).

Every file the search read is a path it showed ([GIT-3](#GIT-3)), whether or not a line in it
matched, so one file the map distrusts makes the whole answer a reference. A file the trust map
withholds is not read, and the answer says one was left out only where it was inside the path
searched. A search pages by the lines it prints, counting a binary file that matches and a file past
the cap as one each, and names the skip that lists the next of them.

**Why.** Finding nothing in a file says something about what it holds too. Labelled by its matches
alone, a search over a file the map distrusts would come back trusted whenever the file happened
not to match, and whether it matched is exactly what that file's author controls. A page counted
in anything other than the lines it printed would repeat or skip one on the next.

`verified-by: bravebot_agent::turn::read_git_searches_the_files_at_a_revision_and_pages_the_lines`
`verified-by: bravebot_agent::turn::a_search_that_read_a_distrusted_file_is_quarantined_matched_or_not`
`verified-by: bravebot_agent::workspace::a_search_pattern_is_held_to_trusted_public_before_anything_is_read`
`verified-by: bravebot_agent::git::search_lists_the_lines_a_pattern_matches_at_a_revision_as_git_grep_does`
`verified-by: bravebot_agent::git::a_search_shows_every_file_it_read_whether_or_not_it_matched`
`verified-by: bravebot_agent::git::a_search_leaves_out_a_withheld_file_and_says_so`
`verified-by: bravebot_agent::git::a_search_pages_by_the_lines_it_prints`

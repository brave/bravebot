---
id: GIT
title: read_git
status: normative
governs:
  - crates/agent/src/git.rs
  - crates/agent/src/workspace.rs
  - crates/agent/src/tools.rs
  - crates/core/src/policy.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Reading a repository's history from the files under its `.git`, without starting git. `query`,
`repository`, `revision`, `path`, `since`, `until` and `count` are routing: the first names which
question is asked, the next three name which repository, which commits and which files it is asked
about, and the rest bound the commits a log lists. There are no content arguments. The result is
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
--abbrev=10` prints them.
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
with a sentence pointing at `run`.

The files a read opens are listed before any of them is decoded, and that list is what the rules
are held against. A file a read opens is on it; a file no read opens is not. That is the
configuration, whatever case its name is written in, the refs a name can reach, which leaves out a
`.lock` and any name with a part starting `.`, the loose objects, and each pack index with the pack
beside it. A listing that runs past the search deadline is declined as a read out of time is.

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
otherwise show, in a diff, a listing or a log narrowed to it, is left out, and the answer says
something was left out.

A rule covering `.git` or any file beneath it keeps the repository closed. read_git reads every
file there or none.

**Why.** Otherwise `show HEAD:.env` is the way round every rule on `.env`. The repository closes
whole because a history with the denied file's objects removed is not one this reader can walk:
which object a file holds is not known until it is read.

`verified-by: bravebot_agent::turn::a_deny_rule_over_a_file_refuses_reading_its_history`
`verified-by: bravebot_agent::turn::a_repository_holding_a_file_a_deny_rule_covers_is_not_opened`
`verified-by: bravebot_agent::workspace::a_repository_a_deny_rule_names_is_not_opened`
`verified-by: bravebot_agent::git::a_withheld_path_is_left_out_of_every_answer_and_the_answer_says_so`

<a id="GIT-5"></a>
### GIT-5: the question is log, show or diff, and anything else is refused by name

A word off the list is refused and the planner is told to use `run`; `status` is told to use `run`
with `git status --short`. A revision form this reader does not implement, `A...B` or `HEAD^@`
among them, is refused by name, and a query given the other shape of revision, one where it takes
two or two where it takes one, is told which query takes it.

**Why.** A guess answers a question the planner did not ask as though it had. `A...B` read as the
forms this reader knows is the range from `A.` to `.B`, and `HEAD^@` is `HEAD^`, so each would
come back as a confident answer to something else. Status reads the index and the working tree,
which this reader does not open.

`verified-by: bravebot_agent::turn::read_git_answers_three_questions_and_points_status_at_run`
`verified-by: bravebot_agent::git::revision_syntax_read_git_does_not_implement_is_refused_by_name`
`verified-by: bravebot_agent::git::a_query_given_the_wrong_shape_of_revision_says_which_query_takes_it`

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

Declined, with a sentence pointing at `run`:

- a `.git` that is a file, or that holds a symbolic link where a read goes
- objects borrowed through `objects/info/alternates`, refs and objects shared through `commondir`,
  and objects replaced or parents grafted through `refs/replace` or `info/grafts`
- configuration that includes another file, sets `core.worktree`, or names a repository format or
  an extension this reader does not know, in the shared file or the per-worktree one
- a ref under `worktrees/` or `main-worktree/`, which lives in another worktree's directory
- symbolic refs that point at each other

Configuration git reads without changing what history means is read, whatever its comments,
quoting, line endings and continued lines.

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

A log lists 20 commits unless the planner names a count, and never more than 200. An answer holds
at most 2,000 lines and each line at most 2,000 characters. A file past 1 MiB is described by its
size, read from the object's header alone. An answer stops at the search deadline. An answer cut
by any of these says it was cut, and one that was not makes no such claim.

**Why.** A repository's history is as large as the repository, and a planner handed part of it as
though it were the whole draws conclusions from what is missing.

`verified-by: bravebot_agent::git::a_log_that_reaches_its_count_stops_and_says_it_was_cut`
`verified-by: bravebot_agent::git::an_answer_past_its_lines_or_a_line_past_its_width_is_cut`
`verified-by: bravebot_agent::git::a_file_past_the_size_cap_is_described_by_its_size`
`verified-by: bravebot_agent::git::a_read_out_of_time_says_so`
`verified-by: bravebot_agent::git::a_listing_or_a_diff_that_fills_the_answer_says_it_was_cut_only_when_more_was_left`

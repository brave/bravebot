---
id: LIST
title: list_files
status: normative
governs:
  - crates/agent/src/glob.rs
  - crates/agent/src/workspace.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Listing what is in a directory. `directory`, `pattern` and `depth` are routing; there are no
content arguments. The result is the paths, or one reference per entry when the planner may not
see them.

## Clauses

<a id="LIST-1"></a>
### LIST-1: a filename is content, so an untrusted listing is quarantined

A listing is trusted only if every file it touched is. A file can be named to read like an
instruction, so the names are treated as content and not shown to the planner.

**The same bytes reach the planner from a language server, and that is not this clause being
contradicted.** [LSP-3](lsp.md#LSP-3) agrees that a filename is content and admits a bounded
disclosure of one anyway, because a location is a name *and* a position and the remedy
[LIST-2](#LIST-2) uses here, a reference the planner passes where it would have typed a path,
carries no position and opens nothing outside the workspace. What that clause owes this one is the
bound written down and its cost enumerated, which is where to read what a name can still carry.

`verified-by: bravebot_agent::workspace::list_enumerates_files_recursively`
`verified-by: bravebot_agent::turn::untrusted_listings_never_reach_the_model`

<a id="LIST-2"></a>
### LIST-2: a quarantined listing returns one reference per entry, not one for the listing

The planner passes the reference where it would have typed a path, and is never told a filename.

That covers what a call says about itself as well as what it returns. A tool asked to act on a
reference reports its result, its refusal and its failure about the reference, never about the file
behind it: a sentence a tool produces is trusted text the planner reads as the driver's own
([LABEL-3](../labels.md#LABEL-3)), so a filename formatted into one is the disclosure this clause
withholds, arriving in the strongest position the design has.

**Why.** One reference for the whole listing would leave the planner holding an address it cannot
use. What came of that in practice was a planner guessing globs to see which came back empty.

`verified-by: bravebot_core::policy::an_entry_reference_names_its_directory_and_never_its_file`
`verified-by: bravebot_core::policy::reserving_the_wrong_number_of_names_is_refused`
`verified-by: bravebot_agent::tools::an_edit_by_reference_that_cannot_be_read_does_not_name_the_file`
`verified-by: bravebot_agent::workspace::a_failure_is_worded_about_the_name_the_caller_may_say`
`verified-by: bravebot_agent::tools::a_deferred_read_that_fails_does_not_name_the_file`
`verified-by: bravebot_agent::tools::a_deferred_read_a_rule_denies_does_not_name_the_file`

<a id="LIST-3"></a>
### LIST-3: the glob is literal and the matcher does not backtrack

The matcher is hand written. `*` and `?` do not cross `/`, `**` does, and a brace group is
expanded into the plain patterns it stands for before the walk rather than matched during it.
An expansion past the cap on how many patterns it may produce in all falls back to matching the
pattern literally, which finds nothing and is reported as finding nothing. Version-control and
build directories are skipped.

A pattern with no `/` is matched against the file name alone. One with a `/` is matched against
the path from the directory the call named and against the path from the workspace root, and a
file either reading matches is kept. `search`'s `include` is read the same way.

**Why.** A backtracking pattern arriving through a turn is a denial-of-service vector. Expanding a
group before the walk is what keeps that bound: each alternative is an ordinary pattern applied
once per path, so a group costs a multiple of the work and never a power of it.

**Why both readings.** A caller naming a directory writes the rest of the path from there, and one
naming none writes it from the root, and planners write both. Read from the root alone,
`*/profile.json` under `projects` selects nothing, which [SEARCH-5](search.md#SEARCH-5) then
reports as a glob to rewrite, and the planner rewrites a glob that was right. Kept to either
reading, every glob that selected a file before still does. A pattern only narrows a walk the
directory and the permission rules have already bounded, so the wider reading reaches no file the
call could not have listed.

`verified-by: bravebot_agent::glob::a_path_pattern_anchors_at_the_root`
`verified-by: bravebot_agent::glob::a_question_mark_matches_one_character`
`verified-by: bravebot_agent::glob::a_double_star_crosses_directories`
`verified-by: bravebot_agent::glob::a_brace_group_matches_each_alternative`
`verified-by: bravebot_agent::glob::an_oversized_expansion_falls_back_to_the_literal`
`verified-by: bravebot_agent::glob::a_pathological_pattern_does_not_blow_up`
`verified-by: bravebot_agent::workspace::the_original_noise_directories_are_still_skipped`
`verified-by: bravebot_agent::workspace::noise_directories_from_other_ecosystems_are_skipped`
`verified-by: bravebot_agent::tools::both_glob_arguments_describe_the_matcher_the_same_way`
`verified-by: bravebot_agent::workspace::a_listing_glob_may_be_written_from_the_directory_it_names`
`verified-by: bravebot_agent::workspace::a_search_include_may_be_written_from_the_directory_it_names`

<a id="LIST-4"></a>
### LIST-4: a truncated listing says it was truncated

Output is capped, and the cap is reported. Reported to the planner whether or not it may read the
names, since a listing handed over as one reference per entry is exactly the case where a notice
inside the body reaches nobody.

**Why.** Silence would let the planner conclude a file does not exist when the answer was cut off.

`verified-by: bravebot_agent::workspace::a_listing_past_the_cap_reports_truncation`
`verified-by: bravebot_agent::workspace::a_listing_within_the_cap_reports_no_truncation`
`verified-by: bravebot_agent::turn::a_quarantined_listing_tells_the_model_it_was_capped`

<a id="LIST-5"></a>
### LIST-5: a listing walks the whole tree unless it is given a depth, and says where it stopped

`depth` is how many directory levels below `directory` are walked. One is that directory and no
further. Absent, the walk reaches every file under it that a permission rule does not cover, which
is what a caller that names no depth gets. A rule keeps a path out of the listing and a directory
out of the walk; see [permissions.md](../permissions.md).

A directory a bounded walk did not descend into is named in the result alongside the files, so
what comes back describes the shape of the tree and not only the part of it that was read. The
pattern does not apply to those: it says which files are wanted, and a directory is where the
answer might be rather than an answer.

A quarantined listing names them the only way it names anything, as one of the references
[LIST-2](#LIST-2) hands over per entry, and each of those says whether it stands for a file or for
a directory the walk stopped at. A reference standing for a directory is refused where a path is
expected, so the difference is enforced and not only stated.

**Why.** Without a bound the only listing on offer is every path at every depth, which in a real
repository is thousands of them. That is paid for in the planner's context, again on every round
that resends it, and again in each delegate handed the same question. The common question is what
a project holds rather than every file it contains, and a listing bounded to one level answers it
in the space of a screen.

**Why the directories.** A bounded listing of files alone describes a tree with no branches. A
planner reading one concludes there is no source directory and looks no further, which is worse
than the listing it was trying to avoid. Whether the names may be read decides who is told them
and not which entries there are, so a quarantined listing of the same directory stops in the same
places. A reference standing for a directory while reading like a file is the other half of that:
it sends the planner to open bytes that are not there, and a write aimed at one asks a person to
endorse a file over a directory they own.

`verified-by: bravebot_agent::workspace::a_listing_given_a_depth_descends_no_further_than_that`
`verified-by: bravebot_agent::workspace::a_depth_limited_listing_names_the_directories_it_stopped_at`
`verified-by: bravebot_agent::workspace::a_pattern_does_not_hide_the_directories_a_bounded_walk_stopped_at`
`verified-by: bravebot_agent::workspace::a_listing_with_no_depth_walks_the_whole_tree`
`verified-by: bravebot_agent::turn::a_bounded_quarantined_listing_hands_over_the_directories_it_stopped_at`
`verified-by: bravebot_core::policy::an_entry_that_is_a_directory_is_not_offered_as_a_file`
`verified-by: bravebot_core::policy::a_directory_reference_is_refused_where_a_path_is_expected`

---
id: EDIT
title: edit_file
status: normative
governs:
  - crates/agent/src/replace.rs
  - crates/agent/src/diff.rs
  - crates/agent/src/notebook.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Replacing an exact passage in a file, or several in one call. `path`, `replace_all`, `cell`,
`cell_action` and `cell_type` are routing; `old_text`, `new_text` and `edits` are content. The result is a confirmation.

## Why this exists rather than a whole-file write

Reviewing a whole file body on a terminal is not review. An edit names the exact passage, so a
person approves a diff of a few lines.

## Clauses

<a id="EDIT-1"></a>
### EDIT-1: an edit refuses rather than guesses

It refuses when the passage is missing, when it occurs more than once without `replace_all`, and
when the file changed since it was read.

**Why.** A guess would change bytes nobody reviewed, and a stale diff would not describe what
actually happens.

`verified-by: bravebot_agent::turn::an_ambiguous_edit_is_refused_without_asking`
`verified-by: bravebot_agent::turn::an_edit_of_a_missing_passage_is_refused`
`verified-by: bravebot_agent::turn::a_stale_edit_is_refused`
`verified-by: bravebot_agent::turn::an_approved_edit_changes_only_the_matched_passage`

<a id="EDIT-2"></a>
### EDIT-2: an edit requires a trusted file

Locating a passage means comparing text, and a comparison is a decision. On a file nobody vouched
for that decision would be taken from bytes an attacker may have written, so it is refused rather
than performed. The route for such a file is a processor and a whole-file write, where nothing is
located and the body is shown to a person in full.

`verified-by: bravebot_agent::turn::editing_an_untrusted_file_is_refused`

<a id="EDIT-3"></a>
### EDIT-3: an edit is approved as a diff, and cannot leave the workspace

The diff compares the whole file, so passages a long way apart are shown as separate changes. A
file whose two sides cannot be matched line by line within the bound [`read_git`](read-git.md)
states for `diff` is shown as a notice that the change is too large to show, with no changes
listed.

`verified-by: bravebot_agent::turn::an_edit_is_reviewed_as_a_diff`
`verified-by: bravebot_agent::turn::a_reviewed_edit_carries_both_sides_of_the_diff`
`verified-by: bravebot_agent::turn::edits_far_apart_in_a_long_file_are_approved_as_two_changes`
`verified-by: bravebot_agent::turn::an_approved_edit_is_recorded_as_endorsed`
`verified-by: bravebot_agent::turn::a_refused_edit_does_not_happen`
`verified-by: bravebot_agent::turn::an_edit_cannot_escape_the_workspace`

<a id="EDIT-4"></a>
### EDIT-4: an edit comes back with the lines it produced

The result carries each changed region of the file, with a few lines either side and their line
numbers, in front of the count of replacements. The regions are found by comparing the file before
and after rather than by locating the new text, so a deletion, an insertion and a `replace_all`
are all shown. Passages a long way apart are separate regions, with a note between them of how many
unchanged lines it stands for, and regions that touch or lie a line apart are one. The regions
together are held to a limit of lines: where they would pass it, the lines either side of each
narrow, down to one, before any change is left out, and where they still pass it the middle is cut
and a note says how many lines it dropped. Two versions too large and too different for the diff to
line up are one region from the first difference to the last. An edit that changes only a line
terminator, or adds or removes the final newline, is compared with the terminators included and
shows the lines it touched.

**Why.** A count of replacements is not a result anybody can check. A session edited eighteen
files on nothing but those counts, never looked at one of them again, and compiled none of it. The
lines around a change answer, in the result the planner already has, the question it would
otherwise spend a round asking, or worse not ask at all. A call with several passages is the same
case once for each, and one span from the first change to the last, cut in the middle, would leave
a passage inside the cut known to have landed only from the count.

**Nothing is declassified for it.** The excerpt is shaped inside the kernel and handed over
labelled, exactly as a read is, so `Policy::present` decides whether the planner sees it. Where the
label is untrusted the result is the count alone, because a quarantined excerpt would take the
confirmation down with it and a planner that cannot be told its edit landed is worse off than one
told only that. [EDIT-2](#EDIT-2) is what makes the file itself trusted by this point.

**The lines are a read of the file, and are scanned as one is.** The excerpt is text from the file
handed to the planner, so before the result is returned the lines it shows are scanned for
credentials under [CRED-15](../credential-protection.md#CRED-15), and only those lines, those of every
region shown: a credential in the unchanged lines between two regions, in the part the excerpt cuts,
or beyond its context, is not in the planner's context.
On a finding the person is asked, as for a read of that file, and one answer covers the file for
this tool and for `read_file` alike. A value the edit's own words wrote is not asked about, since
the planner wrote it and the write was put to the person with it. The edit has landed when the
question is put, so a no does not undo it. The planner is told that the edit landed and that the
lines around it are not shown because the file holds what looks like a credential, and is told
nothing of the finding ([CRED-19](../credential-protection.md#CRED-19)).

`verified-by: bravebot_agent::turn::an_edit_shows_the_lines_it_changed`
`verified-by: bravebot_agent::turn::an_edit_does_not_show_the_planner_a_credential_beside_the_change_unless_the_person_agrees`
`verified-by: bravebot_agent::turn::an_edit_shows_the_lines_around_the_change_once_the_person_agrees_and_asks_once`
`verified-by: bravebot_agent::turn::an_agreement_to_show_a_file_covers_the_lines_an_edit_shows`
`verified-by: bravebot_agent::turn::a_credential_outside_the_lines_an_edit_shows_is_not_asked_about`
`verified-by: bravebot_agent::turn::an_edit_is_not_put_to_the_person_twice_over_a_value_the_planner_wrote`
`verified-by: bravebot_agent::replace::the_runs_an_excerpt_hands_back_carry_the_line_numbers_of_the_file`
`verified-by: bravebot_agent::replace::a_cut_excerpt_hands_back_only_the_lines_it_shows`
`verified-by: bravebot_agent::replace::a_cut_excerpt_says_how_many_lines_it_dropped`
`verified-by: bravebot_agent::replace::an_excerpt_shows_the_changed_line_with_its_neighbours`
`verified-by: bravebot_agent::replace::a_long_span_is_cut_in_the_middle_and_says_so`
`verified-by: bravebot_agent::replace::a_change_of_line_terminator_still_shows_the_line`
`verified-by: bravebot_agent::replace::removing_the_final_newline_still_shows_the_last_line`
`verified-by: bravebot_agent::replace::adding_the_final_newline_still_shows_the_last_line`
`verified-by: bravebot_agent::replace::passages_far_apart_are_each_shown_with_their_own_lines`
`verified-by: bravebot_agent::replace::the_lines_between_two_regions_are_counted_as_unchanged`
`verified-by: bravebot_agent::replace::regions_a_line_or_less_apart_are_one_and_a_wider_gap_is_counted`
`verified-by: bravebot_agent::replace::many_regions_show_less_context_before_any_is_cut`
`verified-by: bravebot_agent::replace::the_context_narrows_to_one_line_when_two_would_still_run_over`
`verified-by: bravebot_agent::replace::a_region_of_exactly_the_limit_keeps_its_three_lines_of_context`
`verified-by: bravebot_agent::replace::regions_that_cannot_all_fit_are_cut_in_the_middle_and_the_cut_is_named`
`verified-by: bravebot_agent::replace::each_region_is_handed_back_with_its_own_first_line_and_the_gaps_are_not`
`verified-by: bravebot_agent::replace::a_deletion_far_from_another_change_is_its_own_region`
`verified-by: bravebot_agent::replace::a_deletion_at_either_end_shows_the_lines_beside_it_and_emptying_a_file_shows_none`
`verified-by: bravebot_agent::replace::a_pair_too_different_to_line_up_still_shows_one_cut_span`
`verified-by: bravebot_agent::turn::every_passage_of_a_several_passage_edit_is_in_the_result`
`verified-by: bravebot_agent::turn::a_credential_beside_a_middle_passage_of_several_is_asked_about`
`verified-by: bravebot_agent::turn::an_agreed_result_shows_every_passage_and_the_credential_beside_the_middle_one`
`verified-by: bravebot_agent::turn::a_credential_between_two_passages_is_not_asked_about`

<a id="EDIT-5"></a>
### EDIT-5: an edit takes the terminator the file already has

When every line terminator in the file is `\r\n`, `old_text` and `new_text` are converted from
`\n` to `\r\n` before the passage is located and before the replacement is written, and text that
already has `\r\n` is left alone. An edit never changes a file's existing terminator. A file that
has no terminator, or only `\n`, is matched as given, and so is a file that mixes the two, since
there is no single terminator to take. An edit that differs from the passage only in its terminators
is refused as unchanged after the conversion, not before it.

The approval states the terminators in words (`line endings: CRLF kept`), because the diff
compares lines without them and a change of terminator alone would otherwise show nothing. It says
nothing when both sides end their lines in `\n`.

**Why.** A planner writes `\n`. Without the conversion, a multi-line `old_text` does not match in a
CRLF file, and a replacement that does match leaves bare `\n` among the `\r\n`: a file that is
inconsistent and a diff that looks right.

`verified-by: bravebot_agent::replace::an_lf_passage_is_found_in_a_crlf_file_and_replaced_with_crlf`
`verified-by: bravebot_agent::replace::a_multi_line_replacement_of_one_line_takes_the_files_terminator`
`verified-by: bravebot_agent::replace::text_that_already_has_crlf_is_not_doubled`
`verified-by: bravebot_agent::replace::every_occurrence_in_a_crlf_file_is_matched_and_converted`
`verified-by: bravebot_agent::replace::an_edit_that_differs_only_in_terminators_changes_nothing_in_a_crlf_file`
`verified-by: bravebot_agent::replace::an_lf_file_is_not_converted`
`verified-by: bravebot_agent::replace::a_file_that_mixes_terminators_is_matched_exactly`
`verified-by: bravebot_agent::diff::a_diff_remembers_the_terminators_of_both_sides`
`verified-by: bravebot_agent::turn::an_edit_to_a_crlf_file_keeps_its_line_endings`
`verified-by: bravebot_agent::confirm::an_edit_that_keeps_crlf_says_so`
`verified-by: bravebot_agent::confirm::a_write_that_swaps_the_terminators_says_which_way`

<a id="EDIT-6"></a>
### EDIT-6: several passages in one file are one call, one diff and all or nothing

`edits` is a non-empty array of `old_text` and `new_text` pairs for the one file `path` names, given
instead of a top-level `old_text` and `new_text` and never beside them. The pairs apply in order to
the text read once, each to the text the pair before it left, so a later pair may match text an
earlier one wrote. `replace_all` applies to every pair. [EDIT-1](#EDIT-1) holds for each pair, and a
call whose pairs together leave the file as it was is refused as unchanged. A refusal names the
pair, as `edit 2 of 3`, and no pair is applied: the file is edited in full or not at all.

The person is asked once, about one diff of the combined result, and [EDIT-4](#EDIT-4) shows each
changed region of it. [EDIT-2](#EDIT-2) and the planner-argument gate apply to every pair.

**Why.** Five changes to a file were five calls, five prompts and five diffs, and no point at which
the person saw the whole change. Editing several files in one call is not offered: each path is a
routing value a person approves alone ([TOOL-2](tool-surface.md)).

`verified-by: bravebot_agent::turn::several_edits_to_one_file_are_approved_once_as_one_diff`
`verified-by: bravebot_agent::turn::a_missing_passage_among_several_changes_nothing_and_names_the_pair`
`verified-by: bravebot_agent::turn::a_later_edit_matches_the_text_an_earlier_one_wrote`
`verified-by: bravebot_agent::turn::edits_beside_a_single_pair_or_cancelling_out_are_refused`
`verified-by: bravebot_agent::turn::a_malformed_edits_argument_is_refused_without_asking_or_writing`

<a id="EDIT-7"></a>
### EDIT-7: the result says what a running language server found wrong

Where a language server for the file's language is already running, the result of an edit adds the
line numbers of the errors it reports, as [LSP-12](lsp.md#LSP-12) specifies: counts and lines in
this repository's words, never the server's, never started by the edit, and given only for a file the
trust map vouches for after the edit.

**Why.** A planner that finds a type error an edit introduced only by running a build spends a round
and a second approval on something the server already knew.

`verified-by: bravebot_agent::lsp::an_edit_reports_the_error_lines_a_running_server_found`
`verified-by: bravebot_agent::lsp::an_edit_starts_no_language_server`

<a id="EDIT-8"></a>
### EDIT-8: a notebook cell is changed by its number, and everything else in the file is kept

On a `.ipynb` file, `cell` replaces passage matching: the number a read showed ([READ-9](read-file.md#READ-9)),
from 1, with `new_text` as the cell's whole source. `cell_action` is `replace` (the default),
`insert_after`, where `cell` is the cell the new one follows and 0 puts it first and `cell_type`
(`code` by default, or `markdown`) says what it is, or `delete`, which takes no `new_text`. `cell` is
refused beside `old_text`, `edits` or `replace_all`, on a file that is not a `.ipynb`, and for a
number the notebook has no cell for. [EDIT-2](#EDIT-2) holds: a notebook nobody vouched for is not
edited, and the planner-argument gate applies to `new_text`. The edit is approved as a diff
([EDIT-3](#EDIT-3)).

The notebook is written back in the layout Jupyter writes, one space of indent, keys in sorted
order and a final newline, so a notebook Jupyter wrote changes only where the edit is, and one laid
out differently is rewritten into that layout and the diff shows it. The other cells, their ids,
their metadata, the notebook's own metadata and its format version are kept. Replacing a code cell
drops that cell's saved outputs and its execution count, which describe source that is no longer
there, and the diff the person approves shows them going. A new cell gets an id no cell has where the
format version gives cells ids.

**Why.** A passage inside a JSON string means escapes, array-of-lines punctuation and a unique match
in a file that repeats itself, and a planner that gets one wrong breaks the notebook. A cell number is
what a read of the notebook shows.

`verified-by: bravebot_agent::turn::an_approved_cell_edit_changes_only_that_cell`
`verified-by: bravebot_agent::turn::a_cell_edit_of_an_untrusted_notebook_is_refused`
`verified-by: bravebot_agent::turn::a_cell_argument_is_refused_beside_a_passage_and_on_a_plain_file`
`verified-by: bravebot_agent::turn::an_insert_at_zero_puts_a_markdown_cell_first`
`verified-by: bravebot_agent::turn::an_insert_after_a_cell_defaults_to_a_code_cell_placed_after_it`
`verified-by: bravebot_agent::turn::a_delete_removes_only_the_numbered_cell`
`verified-by: bravebot_agent::tools::a_malformed_cell_argument_is_refused`
`verified-by: bravebot_agent::tools::a_well_formed_cell_argument_is_accepted_as_what_it_names`
`verified-by: bravebot_agent::notebook::a_replace_changes_one_cell_and_keeps_every_other_byte`
`verified-by: bravebot_agent::notebook::a_replaced_code_cell_loses_its_outputs_and_keeps_its_metadata_and_id`
`verified-by: bravebot_agent::notebook::a_replace_with_the_same_source_changes_nothing`
`verified-by: bravebot_agent::notebook::a_cell_that_does_not_exist_is_refused`
`verified-by: bravebot_agent::notebook::an_insert_goes_after_the_named_cell_with_an_id_nothing_else_has`
`verified-by: bravebot_agent::notebook::an_insert_at_zero_goes_first_and_past_the_end_is_refused`
`verified-by: bravebot_agent::notebook::an_insert_into_a_format_without_cell_ids_adds_none`
`verified-by: bravebot_agent::notebook::a_delete_removes_exactly_the_named_cell`

---
id: EDIT
title: edit_file
status: normative
governs:
  - crates/agent/src/replace.rs
  - crates/agent/src/diff.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Replacing an exact passage in a file, or several in one call. `path` and `replace_all` are routing;
`old_text`, `new_text` and `edits` are content. The result is a confirmation.

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

`verified-by: bravebot_agent::turn::an_edit_is_reviewed_as_a_diff`
`verified-by: bravebot_agent::turn::a_reviewed_edit_carries_both_sides_of_the_diff`
`verified-by: bravebot_agent::turn::an_approved_edit_is_recorded_as_endorsed`
`verified-by: bravebot_agent::turn::a_refused_edit_does_not_happen`
`verified-by: bravebot_agent::turn::an_edit_cannot_escape_the_workspace`

<a id="EDIT-4"></a>
### EDIT-4: an edit comes back with the lines it produced

The result carries the changed region of the file, with a few lines either side and their line
numbers, in front of the count of replacements. The region is found by comparing the file before
and after rather than by locating the new text, so a deletion, an insertion and a `replace_all`
are all shown; a span longer than the excerpt allows is cut in the middle and says how much it
dropped. An edit that changes only a line terminator, or adds or removes the final newline, is
compared with the terminators included and shows the lines it touched.

**Why.** A count of replacements is not a result anybody can check. A session edited eighteen
files on nothing but those counts, never looked at one of them again, and compiled none of it. The
lines around a change answer, in the result the planner already has, the question it would
otherwise spend a round asking, or worse not ask at all.

**Nothing is declassified for it.** The excerpt is shaped inside the kernel and handed over
labelled, exactly as a read is, so `Policy::present` decides whether the planner sees it. Where the
label is untrusted the result is the count alone, because a quarantined excerpt would take the
confirmation down with it and a planner that cannot be told its edit landed is worse off than one
told only that. [EDIT-2](#EDIT-2) is what makes the file itself trusted by this point.

`verified-by: bravebot_agent::turn::an_edit_shows_the_lines_it_changed`
`verified-by: bravebot_agent::replace::an_excerpt_shows_the_changed_line_with_its_neighbours`
`verified-by: bravebot_agent::replace::a_long_span_is_cut_in_the_middle_and_says_so`
`verified-by: bravebot_agent::replace::a_change_of_line_terminator_still_shows_the_line`
`verified-by: bravebot_agent::replace::removing_the_final_newline_still_shows_the_last_line`
`verified-by: bravebot_agent::replace::adding_the_final_newline_still_shows_the_last_line`

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

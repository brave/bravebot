---
id: WRITE
title: write_file
status: normative
governs:
  - crates/agent/src/workspace.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Writing a whole file. `path` and `contents_ref` are routing; `contents` is content. The result is a
confirmation.

## Clauses

<a id="WRITE-1"></a>
### WRITE-1: contents or a reference, never both

`contents_ref` names quarantined content that becomes the whole file. It is **routing**, since it
decides which bytes the write carries, and it is a name the driver handed out rather than anything
derived from content.

Exactly one of the two. Both would leave the driver choosing between text the planner wrote and
bytes nobody has read, and neither names anything to write, so both shapes are refused before
anyone is asked to approve a write.

A blank `contents_ref`, `path` or `path_ref` counts as left out, since an empty string names nothing.
Planners that fill every optional field send the unused ones as `""`, and refusing those as "not
both" gave them a refusal they could not act on. A blank `contents` is still given: it is an empty
file.

**Why.** The worst a wrong reference can do is put the wrong quarantined bytes into a path that
still had to be endorsed on its own.

`verified-by: bravebot_agent::turn::a_quarantined_file_is_rewritten_by_a_processor`
`verified-by: bravebot_agent::turn::a_write_that_names_two_bodies_or_none_is_refused`
`verified-by: bravebot_agent::turn::a_write_with_blank_references_beside_its_path_and_contents_goes_through`
`verified-by: bravebot_agent::turn::a_blank_contents_is_the_body_and_writes_an_empty_file`

<a id="WRITE-2"></a>
### WRITE-2: a reference that names no file is not a destination

Everything a processor produced is such a reference. That refusal is what stops untrusted text
choosing where an effect lands.

`verified-by: bravebot_agent::turn::a_processors_output_cannot_be_a_destination`
`verified-by: bravebot_core::policy::a_reference_that_names_no_file_is_not_a_destination`

<a id="WRITE-3"></a>
### WRITE-3: the planner never chooses a destination on its own

A write needs a person's approval, which mints a single-use endorsement bound to that exact path,
so it cannot be replayed or redirected. Where nobody can be asked, writes are refused rather than
applied unseen. A path the workspace does not reach is refused before anybody is asked, since no
answer makes it reachable ([TRUST-10](../trust-map.md#TRUST-10)).

**Why.** The wrong file destroys work rather than wasting a step.

`verified-by: bravebot_agent::turn::an_approved_write_is_recorded_as_endorsed`
`verified-by: bravebot_agent::turn::a_refused_write_does_not_happen`
`verified-by: bravebot_agent::turn::a_refused_overwrite_leaves_the_original`
`verified-by: bravebot_agent::turn::an_approved_write_cannot_escape_the_workspace`
`verified-by: bravebot_agent::turn::a_write_outside_every_opened_directory_is_refused_before_the_person_is_asked`
`verified-by: bravebot_agent::turn::an_approved_write_does_not_authorise_a_write_somewhere_else`
`verified-by: bravebot_core::policy::an_unendorsed_destination_is_refused`
`verified-by: bravebot_core::policy::an_endorsement_does_not_outlive_the_destination_it_authorised`
`verified-by: bravebot_core::policy::an_endorsement_does_not_authorise_a_different_destination`

<a id="WRITE-4"></a>
### WRITE-4: a write through a reference is always shown

Even where the trust map would not ask. The person approving is shown the path, and the rows of the
change with each character that draws as nothing written out and counted
([PROMPT-13](../prompting.md#PROMPT-13)), since a body a processor wrote is bytes nobody has read.

**Why.** The approval is the only moment the filename is visible to anybody. They own the
directory and are the only party who can say whether that file should be rewritten.

`verified-by: bravebot_agent::turn::every_write_through_a_reference_is_shown`
`verified-by: bravebot_agent::turn::a_write_through_a_reference_says_what_landed_and_that_it_is_done`
`verified-by: bravebot_agent::turn::a_reference_write_is_reviewed_as_a_diff`
`verified-by: bravebot_agent::turn::a_reference_write_spells_what_draws_as_nothing_and_the_file_keeps_the_bytes`

**The exception.** A write whose `contents_ref` is a slot the driver filled from the very path being
written puts the file back as it is, so nobody is asked and nothing is written, provided nothing
has written the file since the slot was read, and no `run` line has started since. The result
tells the planner that the path holds what the reference holds, and the trail has a `write` line
saying so. The test is the driver's own record of which path filled the slot, of the file
authority's revision for that path, and of how many `run` lines had started, all at the moment the
slot was read. They are compared with the destination as a name, with the path's revision now and
with the count now, as numbers. No byte of either side is read. A different path, `./` in front of
the same path, a slot that no file filled (a processor's answer about the file included), a slot
that was not read yet, a slot whose file was written after the slot was read, and a slot read
before any `run` line that has started since are all asked about.

The count is of lines, not of files. A program opens files the line does not name (`cp a X`,
`sed -i X`, a script), so the authority is told of no write to them, and a redirection is the only
form that names its destination. Any line started since the read therefore ends the exception for
every slot read earlier, including a line that wrote nothing. The cost is one question about a
write-back that would have changed nothing, after a `run` line, and the count moves no revision,
so the output of other running commands keeps its label.

`verified-by: bravebot_agent::turn::a_slot_written_back_to_the_file_it_was_read_from_asks_nobody_and_changes_nothing`
`verified-by: bravebot_agent::turn::a_slot_written_back_to_its_file_after_the_file_was_rewritten_is_asked_about_and_restores_it`
`verified-by: bravebot_agent::turn::a_slot_written_back_to_its_file_after_a_run_line_rewrote_the_file_is_asked_about_and_restores_it`
`verified-by: bravebot_core::policy::a_slot_is_not_a_copy_of_its_file_once_the_file_was_written`
`verified-by: bravebot_core::policy::a_slot_is_not_a_copy_of_its_file_once_a_program_started_after_it_was_read`
`verified-by: bravebot_agent::turn::a_slot_written_to_another_path_or_another_spelling_of_its_file_is_asked_about`
`verified-by: bravebot_agent::turn::a_processors_answer_written_to_the_file_it_was_about_is_asked_about`
`verified-by: bravebot_core::policy::only_the_path_a_slot_was_read_from_is_a_write_that_changes_nothing`

<a id="WRITE-5"></a>
### WRITE-5: the result says what a running language server found wrong

Where a language server for the file's language is already running, the result of a write adds
the line numbers of the errors it reports, as [LSP-12](lsp.md#LSP-12) specifies: counts and lines
in this repository's words, never the server's, never started by the write, and given only for a
file the trust map vouches for after the write.

**Why.** A planner that finds a syntax error only by running a build spends a round and a second
approval on something the server already knew.

`verified-by: bravebot_agent::lsp::a_write_reports_the_error_lines_a_running_server_found`
`verified-by: bravebot_agent::lsp::a_write_starts_no_language_server`
`verified-by: bravebot_agent::lsp::a_write_of_quarantined_bytes_reports_no_diagnostics`

<a id="WRITE-6"></a>
### WRITE-6: a replaced file keeps its line endings and its byte order mark

Where `contents` replaces a file the trust map vouches for, the body takes that file's format
before the diff is built: its line endings become the ones the file uses, and it begins with a
byte order mark where the file began with one. A file holding `\r\n` anywhere uses CRLF, and any
other file uses LF. A body that begins with a byte order mark keeps it whatever the file had.

WRITE-1 still says `contents` becomes the file. The normalised text is what lands and is what the
person approves, because the detection happens before the diff: the diff then shows the lines the
write changes rather than every line of the file.

Nothing is converted in three cases. A file nobody vouched for is left alone, since its ending is
a fact about untrusted bytes and converting on it would be the driver branching on content, which
the rule in `agents/AGENTS.md` forbids. A body from `contents_ref` is left alone, since it is
bytes nobody has read. A path with no file yet is written as given.

**Why.** A planner that sends LF text over a CRLF file changes every line, and the approval diff
then shows the whole file as changed. The person has to decline and ask again, or accept a diff
they cannot read and a change of format nobody wanted.

`verified-by: bravebot_agent::turn::a_crlf_file_rewritten_with_lf_contents_keeps_crlf`
`verified-by: bravebot_agent::turn::an_untrusted_pre_image_converts_nothing`
`verified-by: bravebot_agent::turn::a_contents_ref_write_over_a_trusted_crlf_file_keeps_its_own_endings`
`verified-by: bravebot_agent::workspace::a_replaced_files_format_is_detected_and_applied`

<a id="WRITE-7"></a>
### WRITE-7: a person may change the body in their editor before approving it

Where the write prompt is drawn in the terminal and the body is the planner's own `contents`, the
prompt offers a key that opens the body in the person's editor. What they save replaces the body and
is not an answer. The write goes through the credential scan again, a diff is built for the new
text, and the question is put again, even where the path's rule would not have asked. The approval
that follows is bound to the path the first question named, so a change of body moves nothing about
where the write lands. A permission mode that answers writes unasked answers this question as it
answers any other, and leaves a write that would create a credential to the person.

The edit is offered only for the planner's own `contents`, written in a context nothing untrusted
has been shown to. A body from `contents_ref` is never offered one, and neither is text the planner
wrote after it had met untrusted content: what the person kept of such a body is still made of bytes
nobody read, and trusting the saved text would promote them. An edit sent where none was offered
refuses the write.

The saved text is trusted and public: a person wrote it, and it goes into their workspace as the
body it replaces was already going to. A credential the edit adds is refused as one the planner
wrote would be, before the question is put again. The planner is told that the contents the user changed are why, and not what
was found.

The planner is told that the user changed the contents before they were written, and nothing about
what they now say. The text is never put in its context, and it reads the file to find out what the
file holds.

**Why.** A person who wants a small change to a proposed body otherwise declines, types what to
change and waits for a second proposal. The text they save is their own, so trusting it costs what
the approval already cost, and the planner has no need to receive it to carry on.

**Known cost.** Only the terminal offers it. The desktop window, a session in lines, a one-shot run
and an editor hosting a session over the Agent Client Protocol draw the prompt without it. A write
from `edit_file`, from `apply_checkout` and from a manifest run does not offer it either, since the
body of each is derived from a file or fixed in advance rather than written by the planner as
`contents`. The language server's line numbers for the saved file (WRITE-5) are computed from the
person's text and reach the planner as they do for any write.

`verified-by: bravebot_agent::turn::an_edited_write_lands_as_the_person_saved_it_after_being_asked_again`
`verified-by: bravebot_agent::turn::a_second_edit_replaces_the_first_and_the_last_text_saved_is_what_lands`
`verified-by: bravebot_agent::turn::accepting_edits_answers_the_saved_text_as_it_answers_any_other_write`
`verified-by: bravebot_agent::turn::declining_after_an_edit_writes_nothing`
`verified-by: bravebot_agent::turn::the_approval_after_an_edit_covers_only_the_path_it_was_asked_about`
`verified-by: bravebot_agent::turn::an_edit_that_clears_what_made_the_prompt_ask_is_still_put_again`
`verified-by: bravebot_agent::turn::a_value_the_file_already_held_is_written_down_once_however_often_it_is_edited`
`verified-by: bravebot_agent::turn::a_credential_added_in_an_edit_is_refused`
`verified-by: bravebot_agent::turn::a_write_from_a_reference_is_not_offered_an_edit_and_one_sent_anyway_writes_nothing`
`verified-by: bravebot_agent::tools::an_edit_is_offered_only_for_the_planners_own_text_in_a_trusted_context`
`verified-by: bravebot_agent::confirm::a_saved_body_refuses_and_carries_nothing_else`
`verified-by: bravebot_core::policy::an_edited_body_is_trusted_and_public`
`verified-by: bravebot_core::policy::a_proposal_that_was_not_trusted_and_public_is_not_made_so_by_editing_it`

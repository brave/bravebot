---
id: READ
title: read_file
status: normative
governs:
  - crates/agent/src/workspace.rs
  - crates/agent/src/conversation.rs
  - crates/filetype/src/lib.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Reading one file. `path`, `offset` and `limit` are routing; the only content argument is the
`why` every tool takes ([TOOL-5](tool-surface.md#TOOL-5)). The result is the lines, or a reference when the planner may not see them.

## Clauses

<a id="READ-1"></a>
### READ-1: a read the planner may not see does not open the file

Where the result would be quarantined, the path is checked, the file is confirmed present and
text, and the label is fixed from the trust map, all at the moment the planner asks. The bytes are
read only when a processor or a write needs them, and the path is checked **again** then, so a
file that lost its trust in between is read at the lower label.

**Why.** Most of what an agent reads in a directory nobody vouched for is a file it turns out not
to want. Re-checking on the second pass is what stops a reference issued at one label delivering
bytes at another.

`verified-by: bravebot_agent::turn::a_file_the_planner_may_not_see_is_reserved_rather_than_opened`
`verified-by: bravebot_agent::turn::a_page_of_a_file_the_planner_may_not_see_is_reserved_too`
`verified-by: bravebot_core::policy::a_path_that_lost_its_trust_fills_the_slot_untrusted`
`verified-by: bravebot_core::policy::a_read_from_an_unvouched_path_is_untrusted`
`verified-by: bravebot_core::policy::a_read_from_a_trusted_path_is_trusted`

<a id="READ-2"></a>
### READ-2: a long file is paged, and says where to continue from

Reads cap at 500 lines and 2000 characters per line, report the range returned, and give the
offset to continue from.

`verified-by: bravebot_agent::workspace::a_paged_read_is_capped_and_says_where_to_continue`
`verified-by: bravebot_agent::workspace::the_reported_next_offset_returns_the_following_lines`
`verified-by: bravebot_agent::workspace::an_over_long_line_is_shortened_and_counted`
`verified-by: bravebot_agent::workspace::a_multi_byte_line_inside_the_cap_is_returned_whole`
`verified-by: bravebot_agent::workspace::the_cap_keeps_two_thousand_characters_of_a_multi_byte_line`
`verified-by: bravebot_agent::workspace::an_offset_past_the_end_returns_nothing_and_says_the_length`
`verified-by: bravebot_agent::turn::the_model_can_ask_for_a_later_page`

<a id="READ-3"></a>
### READ-3: a file that is not text is reported as binary

Never as a decoding error, which would read as a fault rather than as a fact about the file. A
picture is the exception and is [READ-5](#READ-5): there is something to do with one, so saying
"binary" would be refusing a read that can be answered.

`verified-by: bravebot_agent::workspace::a_binary_file_is_reported_as_binary`
`verified-by: bravebot_agent::workspace::a_paged_read_of_a_binary_file_is_refused`
`verified-by: bravebot_agent::workspace::text_files_are_not_mistaken_for_binary`
`verified-by: bravebot_agent::workspace::an_empty_file_is_not_binary`
`verified-by: bravebot_filetype::lib::a_null_byte_in_the_sniffed_head_is_binary`
`verified-by: bravebot_filetype::lib::only_the_sniffed_head_is_judged`
`verified-by: bravebot_filetype::lib::thirty_percent_control_characters_is_text_and_thirty_one_is_not`
`verified-by: bravebot_filetype::lib::tab_newline_form_feed_and_carriage_return_are_text`
`verified-by: bravebot_filetype::lib::an_empty_file_is_not_binary`

<a id="READ-4"></a>
### READ-4: the planner may choose which file to read

A read changes nothing and is confined to the working directory, so the choice is promoted rather
than put to a person, and every such choice is recorded as a promotion so an audit can separate
the planner's decisions from the user's.

The promotion grants routing and not reach, so the confinement it rests on holds however the result
comes back. A picture read into a `data:` URI is held to the working directory and the directories
the user added exactly as a page of text is: what the file turns out to contain is not a reason to
resolve its path differently.

A path the planner writes with a leading `~` is the person's home directory, expanded before the
confinement is decided ([TRUST-10](../trust-map.md#TRUST-10)), so a file in an opened home is reached
by it and one in a home nobody opened is refused as outside the workspace.

`verified-by: bravebot_agent::workspace::a_leading_tilde_reaches_a_file_in_an_opened_home`
`verified-by: bravebot_agent::workspace::a_leading_tilde_is_refused_when_the_home_is_not_opened`

`verified-by: bravebot_core::policy::a_model_proposal_can_be_promoted_for_a_confined_read`
`verified-by: bravebot_core::policy::a_read_and_a_write_leave_different_trails`
`verified-by: bravebot_agent::turn::a_model_cannot_escape_the_workspace`
`verified-by: bravebot_agent::turn::a_model_cannot_escape_the_workspace_with_a_picture`
`verified-by: bravebot_agent::workspace::only_a_dropped_attachment_may_come_from_outside_the_workspace`
`verified-by: bravebot_agent::workspace::an_attachment_inside_an_added_directory_is_readable`

<a id="READ-5"></a>
### READ-5: a picture is quarantined whatever the trust map says, and leaves quarantine only through `vet_content`

A file whose extension names a picture or a PDF is read as bytes, encoded into a `data:` URI, and
handed back as a reference. The planner is shown one only where `vet_content` let it through, and a
vouched-for directory does not change that: what the trust map answers is whether a file's **text**
may be read, and a picture has none. Nothing offers to vouch for one either, because a yes would
grant the reading of text there is none of.

The reference says what kind of thing it is rather than how many lines it has, because a line count
over base64 describes nothing a reader can act on. Given to `spawn_processor`, it reaches the
processor as a picture in its own part of the request, so the model looks at it rather than reading
base64 as words. The answer is quarantined like any other processor's, per
[PROC-5](../processors.md#PROC-5).

**Why the trust map does not decide this.** A screenshot carries whatever words are in it, so a
picture reaches a planner's context in two ways and no others: a person put it there, pasted or
dropped, which is [PASTE-2](../pasting.md#PASTE-2)'s, or an endorsement of one read let it through
after a check had looked at it. The paste clause names this case directly: never an image a path in model
output named. This is that path, so the picture goes where untrusted content goes, and until an
endorsement says otherwise the one component that looks at it is one that may read untrusted
content.

**The way out is `vet_content`.** [VET-4](vet-content.md#VET-4) gives the planner one picture it
read, once: where a check looked at it and a person handed a copy of it said yes, where auto-vetting
took the check's safe verdict as the answer, or where a run bypassing permissions answered for them
as it answers for text. It never passes through the gate a paste does, and the trust map, the vouch
offer and the read are untouched by it.

**The media type is the driver's.** From a closed table of extensions, shared with the one a drop
uses, never sniffed from the bytes. It ends up in the `data:` URI where it is routing, so deciding
it from content would be deciding a destination from content. A file cannot become a picture by
holding something that looks like one, and a picture cannot become text by being called `.txt`:
either way the extension is what was decided from, and it is part of a path a person can read.

`verified-by: bravebot_agent::turn::a_picture_is_never_shown_to_the_planner`
`verified-by: bravebot_agent::turn::a_picture_is_not_offered_for_vouching`
`verified-by: bravebot_agent::turn::a_processor_is_given_a_picture_as_a_picture`
`verified-by: bravebot_agent::turn::a_picture_a_person_opens_and_lets_through_is_attached_after_the_results`
`verified-by: bravebot_agent::workspace::the_media_type_comes_from_the_extension`
`verified-by: bravebot_agent::workspace::a_file_that_names_no_picture_is_not_one`

<a id="READ-6"></a>
### READ-6: the tool's own description sends a question about change to the token, and says what the token does not settle

`read_file`'s description must say that every read comes back with a change token, that the same
token on a later read means nobody wrote the file in between and a different one means somebody did,
and that a question about whether something changes is therefore answered by reading the file now,
keeping the token, and comparing it with the token from the next look. It must say to take that
baseline **in this turn**, rather than describing how one would be taken. For a file the planner may
not be shown there is no token, and the description must name the size in the reference as what
there is to compare instead.

**It must say what a comparison does not settle**, in the two ways it does not. A change shows up at
the look after it happened rather than when it happened, so the description must have the answer name
the looks it compared and never a time of day, which the planner has no clock for. And a token that
moved says the file was written, not what changed: an identical rewrite moves it, and a change that
leaves the modification time alone moves nothing. Both are [READ-7](#READ-7)'s limits, and an answer
that reports a moved token as a changed file is claiming more than the token said.

**It must say that the next look is the planner's own to arrange.** The description must name
`schedule_next`, say that calling it at the end of the turn has the planner asked again after the wait
with the person's line sent unchanged, and say that this is how a request to be told when a file
changes is answered: read it now, and schedule the look that would catch a change. It must say to do
that rather than to tell somebody to arrange it themselves. Inside a loop the next tick is already the
next look, and the description must say there is nothing to arrange there. And where a look has been
taken and no other scheduled, the description must have the planner say so, or no change reads as a
promise to report the next one.

**Why the planner arranges it rather than the person.** This clause used to have the description hand
over a `/loop` line for the person to type, on the grounds that only a loop outlives a turn. A session
did everything else right, took the baseline, compared two looks, said plainly that nothing was
watching, and then told the person to type `/loop 10s`, which starts nothing: an interval with no
request after it is not a line, so the command refused it and there was no watch. The line was then
corrected and the shape stayed wrong. Handing the work back to somebody who has already asked for it is
an answer nobody wanted, and the tool that fixes it is [SCHED-6](schedule-next.md#SCHED-6): the turn
that took the first look says when to take the next.

**And it must not offer a comparison the planner cannot make.** Nothing in the description may have
the planner read a modification time or a hash in order to compare it with a later one. Neither of
those programs is among the audited few whose output may be read, so what they print comes back as a
reference the planner never sees, and a description recommending that comparison describes work that
cannot be done. That is the whole reason the token is the driver's to compute rather than a program's
to print: the comparison is one only this side of the boundary can hand over.

**Why a clause about wording.** A tool's description is the only instruction the planner reliably
reads, so wording that decides what a turn does is behaviour and belongs in a spec;
[command-line.md](command-line.md) states that generally. The case here is that a read looks like an
answer. Asked when a file last changed, a session read it, described what it held, and left nothing
watching; asked again, it read again and said nothing had changed. Every step of that is a correct use
of this tool, which is why the correction has to be in the sentence the planner reads before it picks
the tool.

**And why the baseline is taken now.** Wording that only named the two lengths of a watch, without
saying what to do about an open-ended request, was worse than the sample it replaced: asked to say
when a file changed, a session answered that a loop would be needed and made no tool call at all,
leaving the person with neither a watch nor the file's contents. A description that routes a question
to a technique has to say which end of it happens in the turn that read it.

`verified-by: bravebot_agent::tools::read_file_sends_a_question_about_change_to_a_token_it_can_compare`

<a id="READ-7"></a>
### READ-7: a read carries a token that differs once the file has been written

Every read the planner is shown comes back with a change token: the file's size and modification time
hashed together into fixed-width hex. Of the **whole file** rather than of the window returned, so a
page and a whole read of the same file carry the same token and asking for less of a file is not
mistaken for the file changing.

**Opaque, because the planner has no clock.** It is told today's date and told not to ask a program
for the time, which [run.md](run.md#RUN-18) states, so a token an hour could be read out of would
invite exactly the invented time that rule exists to prevent. Two tokens can be compared and neither
can be read.

**Shape rather than content.** Nothing derived from the bytes goes into it, which keeps it in the
class of fact a byte count already belongs to: something the driver may hand over about a file whether
or not the planner may see inside it. A digest of the contents would be content, and handing a planner
content-derived bits about a file the trust map quarantines is the thing that arrangement exists to
prevent. It is also why no dependency was added for this: a cryptographic digest is the wrong tool
rather than an unavailable one.

**Taken before the bytes are read.** A file written during the read hands back the new content, and a
token taken afterwards would describe that same new state: the next look would match it and report
that nothing had happened, with the planner holding content it had never seen a token for. Taken
first, the token describes a state at or before the content, so the next look differs and reports a
change that did happen.

**Not an integrity claim, and it must not be described as one.** A write restoring the same bytes
moves the token, and a filesystem that leaves a modification time alone hides a change from it. What
it answers is whether the file looks written-to since the last look, which is the question a planner
asked to say when something changes actually has. [READ-6](#READ-6) requires the description to say
both limits.

**A slot fill does not carry it.** What a slot holds is the file's text, for a processor to work on or
a write to put back, so a note about the file is not part of it. The token is for the planner, which
is the one component that has to compare one look with another.

`verified-by: bravebot_agent::workspace::two_reads_of_an_untouched_file_carry_the_same_change_token`
`verified-by: bravebot_agent::workspace::a_written_file_carries_a_different_change_token`
`verified-by: bravebot_agent::workspace::a_change_token_moves_with_the_modification_time_when_the_size_does_not`
`verified-by: bravebot_agent::workspace::a_change_token_is_the_same_for_different_bytes_of_the_same_size_and_time`
`verified-by: bravebot_agent::workspace::the_change_token_carries_no_time_the_planner_could_read`
`verified-by: bravebot_agent::turn::a_read_hands_the_planner_a_token_that_moves_when_the_file_does`
`verified-by: bravebot_agent::turn::a_read_of_an_empty_file_still_carries_a_token`
`verified-by: bravebot_agent::tools::a_slot_is_filled_with_the_file_and_a_read_also_carries_its_token`

<a id="READ-8"></a>
### READ-8: a repeat of a read the planner still holds is answered with a notice, not the lines again

Where the planner asks for a window of a file that an earlier `read_file` in this conversation
**showed** it, and the file's change token ([READ-7](#READ-7)) is still the one that read carried, the
result is a short notice instead of the lines. It names the window the planner asked for and repeats
the token, so a comparison of looks still has something to compare. Every gate a read passes runs
first, exactly as for a read that returns lines: the capability, the path and the trust label are
checked and observed, and only what comes back differs.

**The same window.** The record is of the file as the trust map keys it, the first line and the line
count the read applied. A different offset or limit is lines the planner has not been shown, whatever
the token says about the file as a whole, and so is a request that comes to the same lines spelled
differently, which is the same window.

**Only a window that was shown.** The record is made by the turn, once the result was presented to the
planner as text. A read that was quarantined, deferred, refused for holding a credential, or that failed
records nothing (so a repeat of it is never a notice about lines the planner never had), and neither
does a read whose result carried no call id to attach the record to.

**It goes with the round that showed it.** An entry stands for lines in a message the planner still
has, and names that message by its position in the conversation, not by the id of the call it answered,
since providers number ids per response and two rounds can carry one. Compaction removes messages from
the request ([COMPACT-6](../compaction.md#COMPACT-6)), and removes the entry with each one, so a read
after a compaction sends the lines again. A resumed conversation starts with no record, which errs toward
sending them.

**The comparison is of metadata.** The token is taken from the file's size and modification time
before any byte is read, so what is compared is a fact about the file's shape and no part of its
text, and the notice is built from that token and the planner's own `offset` and `limit` and nothing
else. A write moves the token and ends the notice. The limits [READ-7](#READ-7) states apply to it
unchanged: a rewrite of the same bytes is a read with lines, and a write that leaves the
modification time and size alone is a notice.

**Why.** A planner that checks a file after each step re-reads it on every round, and every line comes
back each time. A conversation is re-sent whole on every round, so each copy is paid for again on all
the rounds after it, to tell the planner what it already had.

`verified-by: bravebot_agent::turn::a_repeat_read_of_an_untouched_file_is_answered_with_a_notice`
`verified-by: bravebot_agent::turn::a_file_written_since_it_was_shown_is_sent_again`
`verified-by: bravebot_agent::turn::a_different_window_of_the_same_file_is_not_answered_with_a_notice`
`verified-by: bravebot_agent::turn::a_repeat_read_of_a_file_the_planner_may_not_see_is_not_a_notice`
`verified-by: bravebot_agent::conversation::compaction_forgets_the_reads_whose_rounds_it_removed_and_keeps_the_others`
`verified-by: bravebot_agent::conversation::a_kept_read_follows_its_message_to_its_new_position`
`verified-by: bravebot_agent::conversation::a_window_shown_again_is_recorded_under_its_latest_token`
`verified-by: bravebot_agent::workspace::requests_that_come_to_the_same_lines_are_one_window`

---
id: SESSION
title: Sessions and history
status: normative
governs:
  - crates/session/src/sessions.rs
  - crates/tui/src/state.rs
  - crates/tui/src/history.rs
  - crates/session/src/store.rs
  - crates/cli/src/main.rs
  - crates/agent/src/aside.rs
documented-by: docs/website/docs/using/sessions.md
---

## Scope

What is kept between runs: the record of a session so it can be picked up again, the questions a
person asked beside it, the prompts they typed so they can be recalled, and what a session says on
its way out about being picked up. What a resume does to standing permissions is
[prompting.md](prompting.md); what the trail contains
is [trace.md](trace.md). Everything else the command line does is [cli.md](cli.md), which governs
the same file for its own topic.

Everything here describes an ordinary session. A session started with `--incognito` keeps none of
it, and reads all of it: [incognito.md](incognito.md) governs which half is which.

## Clauses

<a id="SESSION-1"></a>
### SESSION-1: a session belongs to the directory it ran in

Records live under one directory per working directory, so the list worth seeing when resuming in
one project is not the list from another. Each session is two files: the record, holding what the
picker shows and what a resume needs, and the trail, appended a turn at a time.

A session is named by a version 4 UUID, and the two files are named after it. Random rather than
counted or clocked, so two sessions cannot collide however many are running, and opaque because the
name is printed on a screen and pasted into a command, and a name built from the time and the
process id would put two facts about the machine somewhere they are nobody's business. Nothing
orders sessions by it; the list is sorted on what each record says it was last written.

`verified-by: bravebot_tui::sessions::sessions_are_written_read_back_and_kept_per_directory`
`verified-by: bravebot_session::sessions::a_session_is_named_by_a_uuid`
`verified-by: bravebot_session::sessions::no_two_sessions_are_given_the_same_name`
`verified-by: bravebot_session::sessions::a_list_puts_the_most_recently_written_session_first`
`verified-by: bravebot_session::sessions::a_working_directory_becomes_one_readable_segment`
`verified-by: bravebot_session::sessions::a_path_with_nothing_in_it_still_names_a_directory`

<a id="SESSION-2"></a>
### SESSION-2: nothing untrusted is ever written down

Every message in the record has already been past the gate that decides what the planner may see,
so what lands on disk is what the planner was allowed to hold: no untrusted bytes, by construction
rather than by filtering. Quarantined content is not written at all, and the trail is labels and
gate names with no content in it.

A rewind point is the one part of a record built from bytes no message carried: it keeps what the
files a turn wrote over held, read off the disk rather than out of the conversation, so a later
session can put them back. Those bytes are written down only where the map that stood before the
turn vouched for the path, which is the map that labelled them. What a file nobody vouched for held
is written down as contents this session did not keep, and a resumed session says that path did not
go back rather than putting it back.

**Why.** A record is read back into a later turn's context. Anything written that the planner could
not have held would enter that context on the next resume, which is the laundering route the whole
design exists to close.

`verified-by: bravebot_tui::sessions::what_a_file_nobody_vouched_for_held_is_not_written_down`
`verified-by: bravebot_tui::sessions::an_answer_the_planner_could_not_have_held_is_not_written_down`
`verified-by: bravebot_agent::turn::the_trail_records_the_slot_and_the_path_rather_than_the_content`

<a id="SESSION-3"></a>
### SESSION-3: the record carries what a resume needs and nothing more

The conversation, the plan each turn worked to, what the session has spent, the branch it ran on,
the questions asked beside the work, and the standing permissions a resume restores. A session can
be named, renaming rewrites the record immediately, a chosen name survives the next turn, and an
empty name is refused.

What the record says about each individual turn is SESSION-23.

`verified-by: bravebot_tui::sessions::renaming_a_session_rewrites_the_record_immediately`
`verified-by: bravebot_tui::sessions::a_chosen_name_survives_the_next_turn`
`verified-by: bravebot_tui::sessions::a_session_can_be_named_before_it_has_a_record`
`verified-by: bravebot_tui::sessions::an_empty_name_is_refused`

<a id="SESSION-4"></a>
### SESSION-4: a title comes from the prompt, and is cut rather than mangled

The first line of what was asked. A long one is cut and says it was, and a prompt with nothing in
it still has a title.

`verified-by: bravebot_session::sessions::a_title_is_the_first_line_of_the_prompt`
`verified-by: bravebot_session::sessions::a_long_title_is_cut_and_says_it_was`
`verified-by: bravebot_session::sessions::a_prompt_with_nothing_in_it_still_has_a_title`

<a id="SESSION-5"></a>
### SESSION-5: everything here degrades to doing nothing

A missing home directory, a full disk, a corrupt record, a stored time in the future: a session
that cannot be written down still runs, one that cannot be read is left out of the list, and a
corrupt history reads as no history rather than as an error.

**Why.** None of this is load bearing for correctness. Failing a turn because a convenience could
not be saved would trade something that matters for something that does not.

`verified-by: bravebot_session::sessions::a_session_from_the_future_is_not_a_crash`
`verified-by: bravebot_session::sessions::a_stored_time_becomes_an_age`
`verified-by: bravebot_tui::persist::a_corrupt_file_reads_as_no_history`
`verified-by: bravebot_tui::persist::no_home_directory_is_not_an_error`
`verified-by: bravebot_tui::persist::the_directory_is_created_on_first_write`

<a id="SESSION-6"></a>
### SESSION-6: a submitted prompt is remembered, and a cancelled one is not

Prompts persist across runs and are capped, consecutive duplicates collapse into one, and a prompt
that was cancelled is removed again. This is input recall, separate from the transcript: cancelled
work already shown stays in the transcript even though its prompt leaves input recall.
Cancellation removes the running submission's claim on its recall entry, including when quitting.
Queued submissions keep their entries. Where consecutive duplicates share an entry, it remains
while any submission it represents has not been cancelled. Cancelling a generated turn removes no
input-recall entry, since that turn did not submit one. A cancellation that removes no entry leaves
the stored history as it stands, because another session may have added to it since this one read it
and writing this session's list back would take those prompts away.

`verified-by: bravebot_tui::sessions::cancelling_removes_only_the_running_prompt_from_recall`
`verified-by: bravebot_tui::sessions::quitting_removes_only_the_running_prompt_from_recall`
`verified-by: bravebot_tui::sessions::cancelling_a_duplicate_keeps_the_earlier_submission_in_recall`
`verified-by: bravebot_tui::sessions::cancelling_queued_duplicates_keeps_recall_until_the_last_submission`
`verified-by: bravebot_tui::sessions::cancelling_a_generated_tick_leaves_input_recall_unchanged`
`verified-by: bravebot_tui::sessions::cancelling_a_generated_tick_keeps_what_another_session_recorded`
`verified-by: bravebot_tui::sessions::cancelled_work_survives_resume_but_leaves_input_recall`

Each is stored with when it was sent and which workspace it was sent from, both of which the search
over the history reads ([terminal-input.md](terminal-input.md#INPUT-20)). A line written before
either was kept is read as a prompt with neither, rather than being dropped or given an invented
time, and is written back out the way it came in.

**Why.** Neither fact can be worked out afterwards: a file's own timestamp says when the newest
prompt was added and nothing about the rest, and a prompt's workspace is gone the moment the session
that sent it ends. Somebody's history is also the one file here whose loss they would notice, so a
format that could not read the previous one would be paid for in exactly the thing this is for.

`verified-by: bravebot_tui::persist::a_prompt_sent_now_is_stored_for_next_time`
`verified-by: bravebot_tui::persist::when_and_where_a_prompt_was_sent_outlive_the_session`
`verified-by: bravebot_tui::persist::a_history_from_an_older_version_is_still_read`
`verified-by: bravebot_tui::state::a_sent_prompt_records_when_and_where_it_was_sent`
`verified-by: bravebot_session::store::when_and_where_a_prompt_was_sent_survive_a_round_trip`
`verified-by: bravebot_session::store::a_line_from_an_older_history_is_still_a_prompt`
`verified-by: bravebot_session::store::a_prompt_with_no_stamp_is_not_given_one_on_the_way_out`
`verified-by: bravebot_session::store::a_prompt_holding_tabs_is_still_one_prompt`
`verified-by: bravebot_tui::persist::a_session_recalls_a_prompt_stored_by_an_earlier_session`
`verified-by: bravebot_tui::persist::an_appended_prompt_is_read_back_next_session`
`verified-by: bravebot_tui::persist::a_cancelled_prompt_is_removed_from_the_stored_history`
`verified-by: bravebot_tui::persist::the_stored_history_is_capped`
`verified-by: bravebot_tui::persist::a_multiline_prompt_survives_a_round_trip_on_disk`
`verified-by: bravebot_tui::persist::saving_replaces_what_was_stored`
`verified-by: bravebot_tui::history::consecutive_duplicates_are_collapsed`

<a id="SESSION-7"></a>
### SESSION-7: recalling a prompt is a mode, and leaving it restores what was being typed

Up walks backwards from the most recent and stops at the oldest, Down walks forwards again, and
leaving the newest entry puts back the half-written line. Submitting leaves the mode, and a prompt
arriving while browsing does not shift the view.

**Why.** Pressing Up out of curiosity must not destroy a line somebody was part way through
writing.

`verified-by: bravebot_tui::history::up_recalls_the_most_recent_prompt_first`
`verified-by: bravebot_tui::history::up_keeps_walking_backwards`
`verified-by: bravebot_tui::history::up_stops_at_the_oldest_entry`
`verified-by: bravebot_tui::history::down_walks_forwards_again`
`verified-by: bravebot_tui::history::down_does_nothing_when_not_browsing`
`verified-by: bravebot_tui::history::leaving_the_newest_entry_restores_the_typed_line`
`verified-by: bravebot_tui::history::submitting_leaves_browsing`
`verified-by: bravebot_tui::history::appending_while_browsing_does_not_shift_the_view`
`verified-by: bravebot_tui::history::a_new_history_is_empty_and_not_browsing`
`verified-by: bravebot_tui::history::an_empty_history_has_nothing_to_recall`
`verified-by: bravebot_tui::history::the_position_counts_from_the_oldest`
`verified-by: bravebot_tui::history::withdrawing_removes_the_cancelled_entry`

<a id="SESSION-8"></a>
### SESSION-8: a session says how to pick it up again as it ends

Leaving prints the command that resumes this session, and the id is the one that fetches it. It is
printed after the terminal is handed back, so it stays on the screen the person is left looking at
rather than going with the interface. A session that never wrote a record prints nothing.

A session that changed its working directory says where it went, because an id is looked up under
the directory the command is run in and the shell reading this line never moved. Where the session
ended where it started, the directory goes unsaid: naming the one somebody is standing in reads as
though something had happened to it.

**Why.** A session is worth resuming far more often than anybody thinks to write its name down
beforehand, and the picker is no use to someone who has already closed the window. Naming a
session with no record behind it would be worse than saying nothing: the command would answer "no
session by that name". Naming one in the wrong directory is worse again, because it does not fail:
the same id in the directory left behind is the same session as it was before the move, so the line
would quietly resume work that stops partway.

`verified-by: bravebot_tui::sessions::a_session_is_named_once_there_is_a_record_to_name`
`verified-by: bravebot_cli::main::a_session_that_stayed_put_is_named_by_its_id_alone`
`verified-by: bravebot_cli::main::a_session_that_moved_says_where_to_resume_it`

<a id="SESSION-9"></a>
### SESSION-9: the theme name is stored globally, like the model

Which theme paints the interface is written under `~/.bravebot` and read back at the next start.
It is not a property of a checkout: the same choice applies in every directory, and an empty or
corrupt file is no choice at all, falling back to `brave`. Custom theme files live beside it, under
`~/.bravebot/themes`, which [terminal-transcript.md](terminal-transcript.md) governs.

**Why.** Asking again in every project for the same preference is answering it repeatedly, and
nothing about a theme depends on which files are open.

`verified-by: bravebot_tui::persist::a_chosen_theme_is_read_back_next_session`
`verified-by: bravebot_session::store::a_stored_theme_is_read_back_without_its_newline`
`verified-by: bravebot_session::store::an_empty_theme_file_is_not_a_choice`
`verified-by: bravebot_session::store::only_the_first_theme_line_is_read`
`verified-by: bravebot_session::store::an_over_long_theme_name_is_not_a_choice`

<a id="SESSION-10"></a>
### SESSION-10: a manifest run is recorded, and cannot be continued

The goal, the proposed plan, the frozen steps, and what each one did are written into the record,
finished or not. The conversation is empty: a session is turns over one conversation, and a
manifest run has none. The picker marks the row and refuses Enter rather than loading an empty
session and asking the model to carry on from nothing. Naming one on the command line prints
what it produced, and still does not continue it.

That print is a report and not a refusal: it goes to stdout and exits successfully, because reading
a run back is what naming one is for, and a session that started a run tells the person this is the
command to read it with. Non-zero is for the failures [cli.md](cli.md) describes, and spending it
here would make the session's own advice look broken to the person who followed it. Forking one is
still refused, since a fork continues a conversation and there is none to continue.

`verified-by: bravebot_tui::sessions::a_manifest_run_is_recorded_and_cannot_be_resumed`
`verified-by: bravebot_tui::resume::a_manifest_session_cannot_be_resumed`
`verified-by: bravebot_tui::resume::a_manifest_run_is_marked_in_the_list`

<a id="SESSION-11"></a>
### SESSION-11: the record says what answered, and what each turn cost

The model the server reported answering with is written down, along with what each turn spent as
well as the total. The breakdown adds up to the total, and a turn that compacted part way through
is charged for that too, since it was asked for in the middle of that turn's work. Something asked
for before the first turn, an aside or a run as the first thing a session does, is charged to a
leading entry ahead of that turn rather than to no turn at all, so the breakdown still adds up to
the total there as well.

Failed and stopped turns charge their latest cumulative progress, including completed delegate
work. Repeated reports replace the previous total. Successful turns charge the outcome alone,
without adding progress again. Progress resets for each turn, so a turn with no completed requests
cannot inherit the previous turn's usage. These totals use the existing session storage.

The name recorded is the one that answered, not the one asked for: an endpoint may serve something
other than the name it was given, and the record is an account of what happened. A record written
before either was kept reads as no model and an empty breakdown, which is not the same as a
session that cost nothing: the total is still there.

**Why.** A transcript is read after the fact to find out why a session went the way it did, and
both questions are unanswerable from a total alone. Twenty even turns and one turn that ran away
come to the same figure and want different fixes. Two sessions cannot be compared at all without
knowing which model produced each, and a global setting read afterwards is today's answer rather
than the one in force at the time.

`verified-by: bravebot_tui::sessions::sessions_are_written_read_back_and_kept_per_directory`
`verified-by: bravebot_tui::state::each_turn_records_what_it_cost_on_its_own`
`verified-by: bravebot_tui::state::an_aside_is_charged_to_the_turn_it_interrupted`
`verified-by: bravebot_tui::state::an_aside_before_the_first_turn_is_charged_to_a_leading_entry`
`verified-by: bravebot_tui::state::a_run_before_the_first_turn_is_charged_to_a_leading_entry`
`verified-by: bravebot_tui::state::clearing_forgets_what_each_turn_cost`
`verified-by: bravebot_tui::sessions::completed_failed_and_stopped_usage_survives_session_storage`
`verified-by: bravebot_tui::app::a_failed_outcome_charges_only_the_latest_progress`
`verified-by: bravebot_tui::app::the_cancellation_path_charges_progress_before_restoring_or_quitting`
`verified-by: bravebot_tui::app::successful_outcomes_replace_progress_and_empty_following_turns_cost_nothing`
`verified-by: bravebot_tui::remote_confirm::cumulative_usage_reaches_the_main_thread_unchanged`

<a id="SESSION-12"></a>
### SESSION-12: the record says where each turn's time went, not only how long it took

Every turn's wall clock is written down split four ways: what was spent waiting on the model, what
was spent running tools, what was spent waiting for the person to answer a prompt, and what is left
over. The four are a partition rather than four independent measures, so the parts account for the
whole and the remainder is meaningful. An approval prompt is drawn from inside a tool call, so what
was spent waiting for a person is taken off the tool figure rather than counted in both.

Delegate inference contributes only where requests overlap the parent's actual collection waits,
with overlapping requests counted once (see [delegation.md](delegation.md)). The resulting
breakdown reaches the same outcome, cumulative progress and session record as the parent's own
measurements; collection does not add a second session charge.

A turn that failed or stopped records its elapsed wall time and the timing breakdown retained from
its progress on the same footing as one that succeeded. A `/compact` asked for mid-turn is charged
to the turn it interrupted, as its tokens are; one asked for before the first turn is charged to
the leading entry its tokens go to. `/status` reports the session total and each part that actually
happened; a part that did not happen is left out rather than shown as zero. A record written before
this was kept reads as an empty breakdown, which is not the same as a session that took no time.

**Why.** A duration alone is unactionable, and the three things it conflates want three different
fixes. A turn that took four minutes on the model, one that took four minutes running a test suite,
and one that took four minutes with a diff on the screen while its user was at lunch are the same
number. Only the last is not the machine's fault, and it is the one a total can never reveal:
without a figure of its own, stalled time is indistinguishable from inference, and so is the
harness's own overhead.

`verified-by: bravebot_tui::state::each_turn_records_where_its_time_went`
`verified-by: bravebot_tui::state::an_aside_charges_its_wait_to_the_turn_it_interrupted`
`verified-by: bravebot_tui::state::an_aside_before_the_first_turn_records_its_wait_ahead_of_that_turn`
`verified-by: bravebot_tui::state::a_failed_turn_still_accounts_for_its_wall_clock`
`verified-by: bravebot_tui::state::unanswered_turns_keep_the_session_clock_and_the_completed_breakdown`
`verified-by: bravebot_tui::state::a_resumed_session_carries_on_from_the_time_it_had_spent`
`verified-by: bravebot_tui::sessions::sessions_are_written_read_back_and_kept_per_directory`
`verified-by: bravebot_session::sessions::a_record_written_before_timing_was_kept_still_loads`
`verified-by: bravebot_tui::status::the_panel_says_where_the_session_spent_its_time`
`verified-by: bravebot_tui::status::a_part_that_never_happened_is_not_reported_as_zero`
`verified-by: bravebot_tui::status::a_session_with_no_turn_yet_reports_no_time`
`verified-by: bravebot_agent::confirm::the_time_a_person_takes_to_answer_is_counted`
`verified-by: bravebot_agent::confirm::every_kind_of_question_is_timed`
`verified-by: bravebot_agent::confirm::a_refusal_is_a_wait_like_any_other`
`verified-by: bravebot_agent::confirm::the_answer_passes_through_untouched`
`verified-by: bravebot_agent::timing::the_remainder_is_what_nothing_else_accounts_for`
`verified-by: bravebot_agent::timing::parts_exceeding_the_whole_do_not_wrap`
`verified-by: bravebot_agent::turn::time_spent_waiting_for_an_approval_is_not_charged_to_the_tool`

<a id="SESSION-13"></a>
### SESSION-13: a session that changes directory is recorded where it moved to

`/cd` moves the record with the working directory, and the session is written there straight away
rather than at the end of the next turn. What was already written stays where it was written: those
turns happened in that directory and are still worth resuming there. A session with nothing written
yet writes nothing, as it does anywhere else, and its destination moves all the same: whatever it
writes later is written where it is working by then.

The trail stays with the turns rather than following the session, since it is appended a turn at a
time beside whichever record was current. A move therefore splits it, and a resume replays the
whole conversation but shows only the gate decisions made in the directory being resumed from. The
conversation does not split: every save writes the whole of it.

**Why.** SESSION-1 keeps one list per working directory, and the record carries the trust map, which
is written in that directory's terms: a relative rule means a path under it. A record left behind
after a move would offer the new directory's answers to somebody resuming in the old one, which is
a yes for a directory nobody was ever asked about. Writing it immediately is what makes the session
resumable in its new home at all: until it is saved there, there is nothing there to find.

`verified-by: bravebot_tui::sessions::a_session_that_changes_directory_is_recorded_where_it_moved_to`
`verified-by: bravebot_tui::sessions::a_session_that_moves_before_anything_is_written_is_recorded_where_it_moved_to`
`verified-by: bravebot_tui::sessions::a_record_written_before_the_first_turn_follows_the_session_when_it_moves`

<a id="SESSION-14"></a>
### SESSION-14: the session someone was just in can be picked up without naming it

`--continue` takes the most recently written session under the working directory, the one the
picker offers first, and picks it up exactly as naming its id would. A manifest run is passed over
rather than refused, since there is no conversation inside one to carry on from. Where the
directory holds no session that can be continued, it says so and fails rather than starting a fresh
one.

**Why.** An id is the answer to "resume that one", and the question people actually have most of
the time is "carry on with what I was just doing". Answering it with an id means finding the line
that printed one, in a terminal that is often the thing that went away. Starting fresh instead
would be answering a different question by discarding the one asked: an empty transcript is
indistinguishable from a session that was lost, and the way to reach an older one is the picker.

`verified-by: bravebot_session::sessions::continuing_takes_the_most_recent_session`
`verified-by: bravebot_session::sessions::continuing_passes_over_a_manifest_run`
`verified-by: bravebot_session::sessions::a_list_with_nothing_continuable_in_it_offers_nothing`
`verified-by: bravebot_tui::sessions::the_session_continued_is_the_one_written_here`

<a id="SESSION-15"></a>
### SESSION-15: the effort level is stored globally, like the model

How hard the model is asked to think is written under `~/.bravebot` and read back at the next
start. It is not a property of a checkout: the same choice applies in every directory, and a blank
file, or one naming a level this program does not define, is no choice at all, leaving the request
to carry none. Asking for no level removes the record rather than writing an empty one, so somebody
who unsets it is back where they were before they ever chose.

**Why.** Asking again in every project for the same preference is answering it repeatedly, and
nothing about how hard to think depends on which files are open. Refusing to store a word this
program does not define is what keeps an edited file from putting an unrecognised level into a
request field.

`verified-by: bravebot_tui::persist::a_chosen_effort_is_read_back_next_session`
`verified-by: bravebot_tui::persist::asking_for_no_effort_is_read_back_as_no_choice`
`verified-by: bravebot_session::store::a_stored_effort_is_read_back_without_its_newline`
`verified-by: bravebot_session::store::a_file_naming_no_level_is_not_a_choice`
`verified-by: bravebot_session::store::only_the_first_effort_line_is_read`

<a id="SESSION-16"></a>
### SESSION-16: session records and directories are private to the user

On Unix platforms, session directories are created with mode 0700, and session files (records,
temporary files, and audit trails) are written with mode 0600. Existing files and directories
are tightened on write.

**Why.** Per SESSION-3, session records hold the full conversation history, prompts, model
responses, file snippets shown to the planner, and accumulated standing permissions. Without
restricted modes, records land at the default process umask (typically 0644 for files and 0755
for directories), leaving private code, potential secrets, and granted permissions readable by
any local account on multi-user machines and shared hosts.

`verified-by: bravebot_tui::sessions::session_records_and_audit_trails_are_written_mode_0600`
`verified-by: bravebot_tui::sessions::pre_existing_session_files_and_directories_are_tightened_on_write`
`verified-by: bravebot_tui::sessions::forking_narrows_the_session_directory_it_writes_into`

<a id="SESSION-17"></a>
### SESSION-17: a transcript can be written out as markdown, inside the working directory

`/export` writes the recounted transcript to a markdown file, at the path named on the line or at
`bravebot-export-<id>.md`. The path is confined to the working directory the way a workspace write
is: `..`, a root and a drive prefix are refused, and containment is then tested against the
canonical path of the deepest directory that exists, so a path leading through a symlink out of
the tree is refused as well. Anything already at the path is refused rather than replaced, a
symlink whose target is missing included. Missing parent directories are created. The file is
written mode 0600, as SESSION-16 writes the record it came from.

Exporting after a resume retains each recorded prompt and safe failure reason, with failed and
cancelled endings distinct and attached to the turn that produced them. Under each prompt, the
export includes its recorded outcome, usage, timing, and task list with each item's saved status.
Missing older measurements and unknown outcomes are omitted rather than printed as measured zero
or success. All of it sits under the prompt's own heading, beside the headings that say who spoke
and how the turn ended rather than in place of any of them.

`verified-by: bravebot_tui::sessions::reopening_keeps_failure_and_cancellation_in_export`

**Why.** The transcript belongs to the person who had the conversation, which
[compaction.md](compaction.md) says in as many words, and without this the only way to exercise
that is to read the record's JSON out of the state directory. The path is typed on the same line
as the command, so it gets the confinement any other path from that line would get; a transcript
carries whatever the session read, and an export that could be steered to an arbitrary path would
be a way to write it anywhere.

`verified-by: bravebot_session::sessions::exporting_a_transcript_is_confined_to_the_project_root`
`verified-by: bravebot_session::sessions::exporting_refuses_traversal_components`
`verified-by: bravebot_session::sessions::exporting_refuses_a_path_through_a_symlinked_directory`
`verified-by: bravebot_session::sessions::exporting_refuses_to_overwrite_an_existing_file`
`verified-by: bravebot_session::sessions::exporting_refuses_a_path_that_is_a_dangling_symlink`
`verified-by: bravebot_session::sessions::an_exported_transcript_is_readable_only_by_its_owner`
`verified-by: bravebot_session::sessions::exporting_creates_intermediate_directories`

<a id="SESSION-18"></a>
### SESSION-18: an interactive session can be forked to explore an alternative path

The `--fork` flag duplicates an existing session into a new session record with its own identifier,
preserving the conversation transcript, spend history, and audit trail while resetting the start
time and marking the title. Manifest runs plan their entire sequence and cannot be forked, matching
the continuation rule in SESSION-10.

**Why.** Exploring an alternative technical path from a shared prefix preserves the expensive
context already built up without polluting the original session. Refusing manifest runs maintains
the invariant that finished autonomous runs have a definite end.

`verified-by: bravebot_session::sessions::forking_a_manifest_session_is_refused`

<a id="SESSION-19"></a>
### SESSION-19: turns can be rewound, on disk and in the conversation together

`/undo` puts the session back where it stood before the most recent turn, and saying it again
goes back another. Every path in the project that a rewound turn wrote through a file tool goes
back to what it held first, and one such a turn created is removed; where two of the rewound
turns wrote the same path, it goes back to what it held before the first of them. The
conversation returns to the snapshot taken before the earliest rewound turn, and with it the turn
count, the spend, the timing, the trust map, the trusted programs, and the transcript. Those
turns' audit lines are dropped, since they decided about turns that are no longer in the
conversation. Their display prompts, outcomes, and task lists are removed with them. Saving and
reopening after rewind must not restore them, and a new turn that reuses a removed turn number
inherits none of its metadata.

`verified-by: bravebot_tui::sessions::reopened_history_stays_rewound_after_another_save_and_new_turn`
`verified-by: bravebot_tui::app::rewinding_reopened_history_removes_outcomes_plans_and_audit_before_reuse`

A rewind that goes back past the session's first turn removes its record rather than
leaving one with nothing in it, and a name the user gave the session before that turn stays with
it: the name was not the turn's to give, so it is not the rewind's to take. The directory the
session was given of its own is not in the project: what a turn wrote there is neither put back nor
counted against the budget below, for the reasons [trust-map.md](trust-map.md) gives.

A standing permission goes back with the turn that granted it. The map and the programs restored
are the ones that stood before the earliest turn being rewound, so a path or a command vouched
for during any of those turns is vouched for no longer, and one vouched for before them is
untouched.

What is kept is bounded twice over. A session remembers its last five turns, and what those turns
wrote over is held to one budget between them rather than to one each: past it the turns furthest
back are dropped whole, and the most recent is kept whatever it cost. Inside a turn the same
budget decides a path: past it the path is still remembered, but what it held is not, and a
rewind treats it as a path that will not go back rather than as a file that was never there. A
path that will not go back is named on the line that reports the rewind, and the rest of the
rewind still happens.

Anything that changes the session outside a turn gives up every point at once: `/clear`,
`/compact`, `/btw`, `/rename`, `/add-dir`, `/cd`, and a shell-mode command, whose writes the
workspace never saw. `/undo` then says there is nothing left to undo rather than rewinding to a
point that describes a different session. Every point goes rather than the most recent alone,
since such a change lands after the most recent point and so before none of them.

**Why.** A turn that went wrong is the case with no clean recovery: `git checkout` takes the
user's own uncommitted work with it, and `/clear` throws away the context that was worth keeping.
Disk and conversation move together because either one alone leaves the transcript describing a
tree that is not there, which is worse than neither. A file that would not go back is that same
disagreement, so it is said out loud rather than swallowed: a person told a turn was undone will
not go looking. The budget exists because the cost is paid by every turn that writes anything,
not by the rare one that is rewound; unbounded, one write of a large file would hold it in memory
for as long as the turn is one a rewind can reach.

**Why more than one turn back.** The turn that just ended is the one least likely to need
rewinding, because it is the one still on the screen. What a person notices late is a mistake
made two or three prompts ago, after approving several diffs in a row, and a session that
remembers only the last turn is no help at exactly that moment. Depth stops at five because every
point holds a copy of the conversation as well as the bytes, and is written after every turn
whether or not it is ever read.

`verified-by: bravebot_agent::workspace::a_rewind_puts_back_what_a_turn_overwrote`
`verified-by: bravebot_agent::workspace::a_rewind_removes_a_file_the_turn_created`
`verified-by: bravebot_agent::workspace::a_path_written_twice_in_a_turn_rewinds_to_before_the_first_write`
`verified-by: bravebot_agent::workspace::taking_the_backups_leaves_the_next_turn_with_none`
`verified-by: bravebot_agent::workspace::a_rewind_names_the_paths_it_could_not_put_back`
`verified-by: bravebot_agent::workspace::a_created_file_already_gone_is_not_reported_as_refused`
`verified-by: bravebot_agent::workspace::a_file_past_the_rewind_budget_is_remembered_but_not_kept`
`verified-by: bravebot_session::sessions::truncating_an_audit_log_removes_events_from_undone_turns`
`verified-by: bravebot_session::sessions::discarding_a_record_leaves_nothing_to_resume`
`verified-by: bravebot_session::sessions::discarding_keeps_a_name_chosen_before_the_turn`
`verified-by: bravebot_tui::app::a_rewind_point_excludes_the_turn_it_undoes`
`verified-by: bravebot_tui::state::clearing_drops_the_transcript_and_what_it_spent`
`verified-by: bravebot_tui::state::closing_the_rewind_window_leaves_nothing_to_rewind_to`
`verified-by: bravebot_tui::state::a_rewind_reaches_past_the_turn_that_just_ended`
`verified-by: bravebot_tui::state::going_back_further_than_the_session_remembers_rewinds_nothing`
`verified-by: bravebot_tui::state::a_path_written_in_two_undone_turns_goes_back_to_before_the_first`
`verified-by: bravebot_tui::state::a_session_keeps_no_more_points_than_it_may`
`verified-by: bravebot_tui::state::one_turns_writes_can_cost_the_session_the_turns_behind_it`
`verified-by: bravebot_tui::state::backups_with_no_point_to_hang_them_on_are_dropped`

<a id="SESSION-20"></a>
### SESSION-20: a question asked beside the work is recorded, and comes back into the view alone

`/btw` asks something over a copy of the conversation and puts neither half into it. Both halves
are written into the record, and a resume puts them back into the mode Ctrl-L opens, which
[watching.md](watching.md) governs. Nothing reads them into a conversation, so a resumed session
carries on from the exchange it had and not from the questions asked beside it.

The answer is written only where the gate that decides what the planner may hold returned it
visible. Where that gate quarantined it, which is a conversation that has met something untrusted,
the question is recorded and the answer is not. A record written before this was kept reads as a
session with no questions beside it, which is what such a session was.

A session that has had no turn yet writes nothing, as it does anywhere else that writes outside a
turn.

**Why.** The reason for keeping them is that the answer exists nowhere else: it is drawn on one
screen, in one mode, and losing it on a resume would mean a person could only read their own
question once. The reason for keeping only what the planner could have held is SESSION-2: a record
holding more than that is a route into a later turn's context, and an aside is written into the
same file as the conversation.

**Why not into the conversation.** Putting it back would undo the whole of what asking it there
achieved. A person asks beside the work precisely so the digression is not in front of the planner
for the rest of the session, and a resume that folded it in would make `/btw` a turn that took one
run to arrive.

`verified-by: bravebot_tui::sessions::a_question_asked_beside_the_work_survives_a_resume`
`verified-by: bravebot_tui::sessions::an_answer_the_planner_could_not_have_held_is_not_written_down`
`verified-by: bravebot_tui::state::a_resumed_session_brings_its_asides_back`
`verified-by: bravebot_agent::turn::an_answer_over_a_trusted_exchange_may_be_written_down`
`verified-by: bravebot_agent::turn::an_answer_over_an_untrusted_exchange_is_shown_and_not_written_down`


<a id="SESSION-21"></a>
### SESSION-21: what a rewind would put back can be read before it happens

`/rewind` with nothing after it lists the points the session can go back to, most recent first
and numbered from one. Each row says how many turns back it is, which turn it would land before,
what that turn was asked, and every path that turn wrote over, or that it wrote over none.
`/rewind <n>` then goes back that many turns, which is what `/undo` said n times does, so what it
puts back is every row from the first down to the one chosen: the list is read in the order it is
printed, and the heading above it says so.

`/rewind` given something that is not a number says what it takes. A number past what the session
remembers rewinds nothing and says how far back it does go, rather than going as far as it can:
somebody who asked for four turns and got two would be reading a tree two turns younger than they
believe it is.

**Why.** A rewind acts the moment it is typed, and it overwrites files, including edits a person
made themselves since the turn. Deciding to run one is deciding about those files, so they have to
be readable first, for the reason a write is shown as a diff before it is approved rather than
reported after. Naming the paths rather than counting them is that same reason carried through: a
count decides nothing.

**Why a word of its own rather than an argument to `/undo`.** A command that takes no argument is
only ever the bare word, which is what keeps `/undo the last thing I asked for` a prompt instead
of a command with a six word argument; [commands.md](commands.md) owns that rule. A surface that
takes a number cannot also have it, so the number is on a word that has one.

`verified-by: bravebot_tui::app::the_bare_rewind_command_asks_for_the_list`
`verified-by: bravebot_tui::app::the_rewind_command_carries_how_far_back_to_go`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_rewind_is_a_prompt`
`verified-by: bravebot_tui::app::undo_with_something_after_it_is_still_a_prompt`
`verified-by: bravebot_tui::app::the_list_names_what_each_point_would_put_back`
`verified-by: bravebot_tui::app::the_list_of_a_session_with_no_points_says_there_is_nothing`
`verified-by: bravebot_tui::state::going_back_further_than_the_session_remembers_rewinds_nothing`


<a id="SESSION-22"></a>
### SESSION-22: the turns a rewind can reach are kept with the record

A session's rewind points are written into its record along with the conversation, and a resume
brings them back: the exchange each one goes back to, the counts, the trust map and the programs
that stood before its turn, what that turn was asked, and what its writes overwrote. `/undo` and
`/rewind` after a resume reach the same turns they reached before the program was closed.

What a path held is written base64 in the record, so the record carries the rewind budget as well
as the conversation. Only for a path the map that stood before the turn vouched for: SESSION-2 is
why one it did not vouch for is written down as a path whose contents this session did not keep,
which the session itself still holds and can still put back. Paths inside the project are recorded
relative to it and come back under the directory the resumed session works in, as trust rules do.

Only the points a rewind can still reach are written. A record holds what the session holds, so a
point that ages out of the session's depth or budget, and every point given up when something
changes the session outside a turn, is gone from the record at the next write. What a turn
overwrote therefore leaves the record a few turns after that turn rather than accumulating for the
life of the session. SESSION-16's 0600 covers it while it is there, as it covers the conversation
beside it.

Anything read back that this build cannot make sense of means the path will not go back: a word
for what was there that it does not know, and contents that will not decode, both land there
rather than on the path having been absent. A resumed point's place in the transcript is worked
out from explicit turn boundaries rather than a stored display index, since the transcript a
resume draws is not the one the point was taken against. Older records without those boundaries
use the conversation captured by the point; no missing outcome is inferred from that content.

**Why.** A mistake is often noticed after closing the program and opening it again, which is the
same case SESSION-19 exists for a few minutes later. A resume that brought back the transcript
describing what those turns wrote, and nothing to put any of it back, made the record a
description of a tree it could no longer restore.

**Why in the record rather than beside it.** The record already holds the conversation, which is
the bulk of a point, and it is written atomically and read privately (SESSION-16). A file of its
own would need both of those again, and would let a session exist whose record and whose rewind
points disagree about how many turns it has had.

**Why the safe direction is "will not go back".** The two states a rewind can be wrong about are
not symmetrical. Treating a path it cannot restore as one that was never there would delete a
file somebody was working on; treating it as one that will not go back leaves the file alone and
names it on the line that reports the rewind.

`verified-by: bravebot_tui::sessions::a_rewind_point_survives_being_written_and_read_back`
`verified-by: bravebot_session::sessions::a_kept_file_this_build_cannot_read_will_not_go_back_rather_than_being_deleted`
`verified-by: bravebot_tui::state::a_restored_point_finds_its_place_in_the_transcript_it_comes_back_into`

<a id="SESSION-23"></a>
### SESSION-23: the record says where each turn began and ended, and what came of it

A record keeps, for every turn, its number, the prompt as it was shown, what came of it, and which
messages of the conversation belong to it. A turn that failed or was cancelled keeps its own task
list, spend, timing and audit lines through saving and reopening, with no final answer and no
planner message of its own needed to hold them. A recorded failure carries only the reason the
interface composed, never a message from the backend. A prompt handed back to the editor on
cancellation stays out of the transcript on resume while that turn's spend and timing stay with its
number; whether those words remain available to recall is SESSION-6, and the two are independent of
each other. None of this is sent to the planner.

**Why.** Turn boundaries cannot be recovered from the conversation afterwards, because a
user-role message is as likely to be loaded context, a correction or a shell line as a prompt.
Guessing them charges one turn's spend to another and shows one turn's plan under the next, which
is worse than a resumed session that says nothing about either. The reason is composed rather than
copied because a record is read back into a later turn's context, which is the route SESSION-2
closes.

`verified-by: bravebot_tui::sessions::reopening_keeps_exact_prompts_and_turn_count`
`verified-by: bravebot_tui::sessions::reopening_keeps_task_ownership_and_recorded_measurements`
`verified-by: bravebot_tui::sessions::failure_after_work_keeps_its_prompt_and_safe_reason_without_changing_context`
`verified-by: bravebot_tui::sessions::hidden_cancellation_then_corrections_keeps_plan_ownership`
`verified-by: bravebot_tui::sessions::processor_cancellation_preserves_its_plan_and_measurements_on_resume`
`verified-by: bravebot_tui::sessions::reopening_does_not_restore_an_unsent_prompt`
`verified-by: bravebot_tui::sessions::a_request_after_resume_excludes_the_display_failure`

<a id="SESSION-24"></a>
### SESSION-24: a resumed transcript puts the prompt back where it was sent

The turn records where the submitted prompt entered the conversation, and a resume replaces that
one message with the prompt as it was shown. Context read for the turn, corrections sent while it
ran, and what a delegate reported keep their own order within the turn. A context read that fails
before the prompt is appended keeps both the prompt and whatever context was recorded before it. A
delegate's prompt position belongs to its own conversation and never stands for the parent turn's.

**Why.** The prompt is not always the turn's first message, since context named on the line is read
into the conversation ahead of it. A resume that assumed it was first would show that context as
the words the person typed and lose the words they did type, and a transcript that misreports the
question is worse than one that omits it.

`verified-by: bravebot_tui::sessions::context_before_prompt_keeps_each_message_once`
`verified-by: bravebot_tui::sessions::context_loading_reports_the_submitted_prompt_position`
`verified-by: bravebot_tui::sessions::partial_context_failure_preserves_the_context`
`verified-by: bravebot_tui::sessions::context_loading_failure_preserves_partial_context_and_prompt`
`verified-by: bravebot_tui::sessions::reopening_keeps_answers_after_prompts_with_internal_prefixes`
`verified-by: bravebot_agent::shared::only_the_parent_reports_its_prompt_position`

<a id="SESSION-25"></a>
### SESSION-25: a record that never said where its turns began is not given boundaries now

Such a record keeps the turn count it states, and its user-role messages establish nothing: loaded
context, a correction and a shell line carry the same role as a prompt. A resume leaves those
messages belonging to no turn, keeps the measurements and task lists the record does hold, and
attaches no turn's metadata to a message it guessed was a prompt. An outcome the record does not
state stays unknown and a measurement it does not hold stays absent, rather than being shown as
success or as a measured zero. Saving preserves all of this: only turns taken after the resume gain
boundaries.

**Why.** Somebody's own sessions are the ones this is read back for, so a format that could not
read what is already on their disk would be paid for in exactly the thing it is for. A boundary
guessed from a role is wrong on precisely the long sessions worth resuming, the ones that loaded
context and were corrected, so leaving the older half of a session unattributed costs a heading
where guessing costs the numbers.

`verified-by: bravebot_tui::sessions::legacy_context_keeps_measurements_without_guessing_turn_ownership`
`verified-by: bravebot_tui::sessions::old_history_keeps_unknown_outcomes_and_missing_measurements`

<a id="SESSION-26"></a>
### SESSION-26: a worker that loses its conversation keeps the turns taken before it

The turns before the loss keep their prompts, outcomes and measurements, and the messages they
claimed are no longer claimed: those places belong to a conversation that is gone, and the one the
session holds now starts again from nothing. A rewind across the loss brings back the earlier
conversation and the places in it together.

**Why.** A turn number is what the person saw and what their spend is filed under, so losing the
conversation must not renumber the session. Letting the old places stand instead would hand the
first turn's boundaries to whatever the new conversation puts in the same positions, which reads as
a transcript of work nobody did.

`verified-by: bravebot_tui::sessions::a_lost_conversation_keeps_turn_identity_and_can_be_rewound`

<a id="SESSION-27"></a>
### SESSION-27: what each turn spent can be asked for while the session is running

A word asks what the session has cost and is answered with the total, and under it one figure per
turn with that turn's share of the total beside it. What was spent before the first turn is
reported too and is not given a turn's number, since no turn did it. A record that holds a total
and no breakdown says the breakdown is missing, which is not the answer a session that has spent
nothing gives.

The figures are tokens. Nothing here states a price: no model listing carries one, and a prompt a
service answered out of its own cache is billed at a fraction of a fresh one while the breakdown
keeps no cache split per turn, so a figure in money would be composed here rather than measured.
That the figures are not a bill is under Known costs.

**Why.** The breakdown exists to answer "where did it go", and a figure only a file on disk holds
answers nobody. A total cannot tell twenty even turns from one that ran away, and the share is
what makes the second one visible without the reader dividing each row by the total themselves.

`verified-by: bravebot_tui::state::what_each_turn_spent_is_reported_turn_by_turn`
`verified-by: bravebot_tui::state::each_turn_is_reported_as_a_share_of_the_session`
`verified-by: bravebot_tui::state::what_was_spent_before_the_first_turn_is_not_reported_as_a_turn`
`verified-by: bravebot_tui::state::a_total_with_no_breakdown_does_not_read_as_a_session_that_spent_nothing`
`verified-by: bravebot_tui::state::a_session_that_has_spent_nothing_says_so`
`verified-by: bravebot_tui::app::typing_the_cost_command_reports_rather_than_prompting`

## Known costs

- **Two working directories can share a session store.** The directory name is derived by mapping
  every character outside a small set to `-`, which is lossy, so `/a/b`, `/a-b` and `/a b` all
  reduce to the same name. Nothing re-checks afterwards: the listing reads every record in that
  directory without comparing the path recorded inside it. Since a resume restores standing
  permissions, permissions granted in one of those directories would be offered in another.

  The record does hold the true path, so the fix is to filter on it. `two_directories_do_not_share_a_key`
  does not cover this: it compares `/a/one` with `/a/two`, which differ before the mapping is
  applied.

- **A record grows with what its turns wrote over.** Every rewind point carries a copy of the
  conversation and the bytes the turn overwrote, and the whole record is rewritten after every
  turn. A session whose turns rewrite large files therefore writes a large record repeatedly,
  whether or not anything is ever rewound. The memory budget bounds it, and nothing smaller does:
  storing diffs between consecutive points rather than copies would, at the cost of a mechanism
  that has to be right about every write a turn makes.

- **A record holds what the last few turns overwrote inside a directory somebody vouched for.** A
  turn that replaces such a file puts that file's previous contents in the record, and vouching for
  a directory is not saying that every file in it is worth copying anywhere: overwriting a file of
  credentials inside a project the user trusts copies them out of it and into the state directory.
  They leave the record once the point ages out, and 0600 covers them while they are there, but a
  session that ends on such a turn leaves them in its record until the record is deleted. Nothing
  here distinguishes a file worth keeping from one that is not, because nothing here reads the
  bytes.

- **A resumed session cannot rewind a file nobody vouched for.** What such a file held is kept in
  memory for the session that took the backup and left out of the record, so after a resume that
  path is reported as one that did not go back, beside the paths whose contents were past the
  budget. The turn is still undone in every other respect and the file is left alone rather than
  deleted. The alternative is those bytes on disk, which is what SESSION-2 refuses.

- **A rewind sees file-tool writes and nothing else.** The backups are taken inside the workspace,
  so a turn that changed a file by running a program instead (`run`, per [run.md](tools/run.md))
  leaves nothing to put back. `/undo` still reports one turn rewound, and the conversation is,
  but that program's changes stay on disk. Covering them would mean snapshotting the tree around
  every command rather than around every write, which is a different and much larger mechanism.

- **A rewind overwrites edits made since the turn.** What goes back is what the path held before
  the turn wrote to it, so a file the user edited themselves in between loses that edit. Nothing
  compares the file against what the turn left there, and nothing asks first.

- **The per-turn breakdown counts what a turn sent, which is not what it cost.** A cached prompt
  token is charged at a fraction of a fresh one, so two turns recorded at the same figure can differ
  about tenfold in money. How much of a prompt a service answered out of its own cache is a figure
  [backends.md](backends.md) carries out of a reply that states one, and it is not kept here. What is
  written stays comparable across turns and across sessions, which is what the record is read for,
  and it cannot be read as a bill. Keeping the split would mean a resumed session reporting a cache
  it never used; the figures are on the status panel for the turn that just ran, where they are a
  measurement rather than a history.

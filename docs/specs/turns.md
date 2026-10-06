---
id: TURN
title: What bounds a turn
status: normative
governs:
  - crates/agent/src/turn.rs
documented-by:
  - docs/website/docs/troubleshooting.md
  - docs/website/docs/reference/cli.md
  - docs/website/docs/using/transcript.md
---

## Scope

How long a turn may go on, what happens when it does not stop, and what is said when it goes on
without producing anything or ends without checking anything. What completed requests cost survives
a later failure or stop. What happens when the model's reply says nothing at all, and when it runs
into its output ceiling.

## Clauses

<a id="TURN-1"></a>
### TURN-1: a bounded turn loses its tools rather than ending

A turn carries a round limit or carries none. On the limiting round the next request offers no
tools, the planner is told it has none left, and it answers with what it has. A call it asks for
anyway is dropped rather than run.

**Not a safety property.** A gate refuses on the thousandth round what it refuses on the first.
This is a bound on futility: in a directory nobody vouched for, a planner looking for a file it
cannot name will try glob after glob for as long as anyone lets it. Do not cite this clause as a
containment measure.

**Compaction is not this bound.** Compaction bounds how full the context is, not how long a turn
runs. The glob loop above stays under any budget indefinitely, so compaction is what lets it run
forever rather than what stops it. The two cover opposite failures: compaction stops a turn dying,
this stops a turn never dying.

`verified-by: bravebot_agent::turn::a_turn_that_keeps_calling_tools_is_made_to_answer`
`verified-by: bravebot_agent::turn::calls_made_after_the_budget_is_spent_are_not_run`
`verified-by: bravebot_agent::turn::a_turn_is_not_cut_off_after_a_fixed_number_of_rounds`

<a id="TURN-2"></a>
### TURN-2: the bound belongs to the caller, and an interactive turn has none

Who is watching decides the limit, so the caller sets it. Every caller with a person in front of
it passes none: the terminal, the line-mode session, and the desktop window. That person sees what
a turn is doing and their stop reaches it mid-round, so any number would only interrupt work that
was going fine. A one-shot `-p` run and a manifest run pass the default 200, because an unwatched
loop has nothing else to end it.

The default is bounded, because a default cannot know whether anybody is watching and being wrong
that way is the cheaper mistake. This was 40 everywhere, which interrupted real work in a large
repository. So an interactive caller states its bound rather than leaving it to the default:
silence is the answer for a caller nobody is watching, and a watched turn takes it by omission.

`verified-by: bravebot_agent::turn::an_unbounded_turn_is_never_made_to_answer`
`verified-by: bravebot_agent::turn::a_turn_that_keeps_calling_tools_is_made_to_answer`
`verified-by: bravebot_ui_bridge::rounds::a_desktop_turn_is_not_cut_off_at_the_bound_an_unwatched_run_carries`

<a id="TURN-3"></a>
### TURN-3: a turn that has written nothing for long enough is told so

Where a write is possible, a turn has gone a set number of rounds, and no write has been asked for
since a turn last ended with an answer, the driver says so once, at the end of a round, and the
turn carries on with its tools. The line is a nudge, not a bound: nothing is taken away, nothing is
refused, and a planner that keeps reading keeps reading.

**The number is measured, not chosen.** It was fifteen, and a run with the prompt and the line
together wrote its first file on round sixteen: the line working, and the paragraph asking for the
same thing not. Where a planner is reading past the point it could have written, the number is too
high.

**A different futility from [TURN-1](#TURN-1).** That one is about a turn which never ends. This
one is about a turn which ends having only understood: a planner that maps a repository before
changing anything is doing real work, and it still leaves nothing behind when somebody stops it,
which is the ordinary way a person finds out a turn went wrong.

**Said once, and conditionally worded.** Repeating it every round spends a request to say what is
already in the conversation. The driver cannot tell a task that asks for a change from one that
asks a question, and must not try: it knows only that rounds have gone by with nothing written,
so the line says what to do if a change was wanted and to carry on if it was not.

**A requested write counts, not a completed one.** A write the user refused is a planner that
tried to deliver, and telling it to start delivering would answer something nobody asked.

**Counted since the last answer, not since the turn began.** A stop is usually not the end of a
task: the next prompt is `continue`, and a change the stopped turn asked to write is the change
being continued. An answer usually is the end of one, so the turn after it starts from nothing
written. The count belongs to the conversation, so a session resumed after a stop keeps it. A
fork cut in front of one of the parent's prompts starts from nothing written, because the count
describes the parent's last turns rather than the ones in front of the cut: starting clear costs
at most a nudge the kept turns did not need, where starting set could withhold one they did.

`verified-by: bravebot_agent::turn::a_turn_that_writes_nothing_for_long_enough_is_told_so`
`verified-by: bravebot_agent::turn::a_turn_that_has_written_is_not_told_to_write`
`verified-by: bravebot_agent::turn::a_turn_after_a_stopped_turn_that_wrote_is_not_told_to_write`
`verified-by: bravebot_agent::turn::a_turn_after_a_completed_turn_that_wrote_is_still_told_to_write`
`verified-by: bravebot_agent::conversation::a_restored_conversation_remembers_a_write_asked_for_since_the_last_answer`
`verified-by: bravebot_ui_bridge::fork::a_write_the_parent_asked_for_does_not_survive_a_cut`

<a id="TURN-4"></a>
### TURN-4: a turn that changed files and ran nothing says so, to both parties

Where a run is possible, files have changed and no program has been run, the planner is asked once
whether any of it runs, a set number of rounds after its first write, and pointed at a checker
delegate for a long log. When the turn ends in that state the person is told plainly that nothing
was built or tested.

**Two audiences, two moments.** The planner can still act, so it is asked while the turn is going;
the person is about to act on a diff, so they are told at the end. Neither is a reproach: plenty of
turns have nothing to build, and both lines say what happened rather than what should have.

**Counted from the write.** Before a file changes there is nothing to run, so a turn that spends
twenty rounds reading is not asked about a build it has no reason to have done.

**However the turn ended.** A stop and a failed request leave the same changed files on disk as an
answer does, so the person is told on all three endings. Being stopped with nothing compiled is the
ending a person is least able to spot for themselves: no outcome is drawn to read it out of.

**What happened, not what was asked for.** Both lines are about the workspace, so both count a
write that landed and a program that started, never the call the planner made. A write the person
declined and a write plan mode refused leave nothing to build, and a run the person declined
builds nothing. This is the opposite of [TURN-3](#TURN-3)'s nudge and for the opposite reason:
that line is about what the planner tried to do, and these two are about what the person is
about to act on.

`verified-by: bravebot_agent::turn::a_turn_that_writes_without_running_is_asked_about_it`
`verified-by: bravebot_agent::turn::a_turn_that_wrote_and_ran_is_not_asked_about_it`
`verified-by: bravebot_agent::turn::a_write_the_person_refused_is_not_reported_as_a_change_that_was_never_built`
`verified-by: bravebot_agent::turn::a_write_plan_mode_refused_is_not_reported_as_a_change_that_was_never_built`
`verified-by: bravebot_agent::turn::a_run_the_person_refused_leaves_the_change_reported_as_never_built`
`verified-by: bravebot_agent::turn::a_turn_offered_no_run_is_not_reported_as_a_change_that_was_never_built`
`verified-by: bravebot_agent::turn::a_turn_stopped_before_any_write_is_not_told_a_change_was_never_built`
`verified-by: bravebot_agent::turn::a_turn_stopped_after_a_write_is_told_the_change_was_never_built`
`verified-by: bravebot_agent::turn::a_turn_that_failed_after_a_write_is_told_the_change_was_never_built`

<a id="TURN-5"></a>
### TURN-5: completed work remains charged when a turn fails or stops

The turn reports cumulative usage after completed planner, processor and compaction calls, and
when delegates are collected. Each report replaces the previous total. A delegate retains its own
progress until collection, so it cannot overwrite the parent's total. Collection adds either its
successful outcome or its retained progress on failure, once. A failed or stopped parent collects
outstanding delegates before returning and reports the resulting total and elapsed timing.

A completed reply also contributes valid reported usage when its content is rejected by the
backend, or when cancellation arrives after protocol completion but before transport EOF.
Planner, processor and compaction errors carry that usage into the cumulative total exactly once.
Known usage from the final planner attempt also updates the last measured prompt size, even when
its reply is rejected. Costs retained from earlier completed retry attempts enter the cumulative
total once, but do not replace the final prompt size or supply a measurement for an unfinished
attempt.
Unknown usage adds nothing; no estimate is made for a failed or unfinished request. A later call
cannot inherit a previous call's retained usage. These totals use the existing session storage
and resume path.

Timing measures elapsed time: overlapping delegate requests count once and only during parent
collection waits. [delegation.md](delegation.md) defines the wait accounting.
Raw backend errors remain outside planner context and user-facing history.

**Why.** A later error or stop does not undo the cost of requests that already finished.

`verified-by: bravebot_agent::turn::planner_and_processor_progress_survives_failure`
`verified-by: bravebot_agent::turn::planner_and_processor_progress_survives_cancellation`
`verified-by: bravebot_agent::turn::compaction_progress_survives_failure`
`verified-by: bravebot_agent::turn::compaction_progress_survives_cancellation`
`verified-by: bravebot_agent::turn::successful_parents_collect_outstanding_delegate_usage_once`
`verified-by: bravebot_agent::turn::failed_parents_collect_outstanding_delegate_usage_once`
`verified-by: bravebot_agent::turn::stopped_parents_collect_outstanding_delegate_usage_once`
`verified-by: bravebot_agent::turn::the_last_request_keeps_elapsed_time_on_failure_and_cancellation`
`verified-by: bravebot_agent::shared::what_a_delegate_has_spent_is_not_reported_as_what_the_turn_has`

`verified-by: bravebot_agent::turn::planner_retry_costs_do_not_replace_the_last_prompt_measurement`
`verified-by: bravebot_agent::turn::completed_empty_reply_keeps_reported_usage_on_failure`
`verified-by: bravebot_agent::turn::rejected_subrequests_are_counted_once_when_the_parent_succeeds`
`verified-by: bravebot_agent::turn::rejected_processor_keeps_completed_usage_when_the_parent_fails`
`verified-by: bravebot_agent::turn::rejected_compaction_keeps_completed_usage_when_the_parent_fails`
`verified-by: bravebot_agent::turn::completed_stream_keeps_usage_when_cancelled_before_socket_closes`
`verified-by: bravebot_tui::sessions::malformed_completed_usage_survives_turn_storage_and_resume`

<a id="TURN-6"></a>
### TURN-6: an empty reply is asked about once before it ends a turn

When the planner finishes a reply with no text and no calls, the driver adds a line to the
conversation saying so and asks again, and the person is told nothing. A second empty reply in a
row ends the turn as a failure. An answered request in between starts the count again, so a long
turn can come back from more than one.

**Asked with a line, not sent again unchanged.** An empty reply comes from the conversation rather
than the connection: the model read the request and ended its reply without writing anything, which
is seen most often after tool results with text following them. The same request sent again tends
to come back empty too, and a line saying so gives the model something to answer. The line is marked
as the system's, like every other line the driver adds, so it does not read as the person changing
the task.

**Once, because twice is an answer.** A model that says nothing when asked to carry on has nothing
to say here, and asking a third time would spend a request per round finding that out.

**Only a finished reply with nothing in it.** A reply that was cut off, could not be decoded, or
was refused is not this. Each has its own ending, one cut off at the ceiling has
[TURN-7](#TURN-7), and asking a model to carry on from something it never finished saying would
answer the wrong failure.

**Deciding on it reads nothing untrusted.** The reply is the planner's own output, and the
planner's context holds nothing untrusted, so what it produced is trusted
([LABEL-8](labels.md#LABEL-8)). The driver already decides from that reply whether the turn goes
on, by whether it asked for a call, and whether it said anything at all is the same kind of fact.

An ordinary ending does not discard session decisions. After child cleanup, the engine returns
current file decisions, exact command approvals, live command advice and live credential-exposure
answers alongside either an outcome or an error. The plain CLI, terminal worker and desktop bridge
adopt these before handling the ending. Early context-loading errors follow the same rule.
Cancellation before effects keeps existing decisions. Delegates return exact command approvals
on ordinary endings; file authority remains shared and child advice remains local.

`verified-by: bravebot_agent::turn::retention::early_loading_errors_return_current_decisions`
`verified-by: bravebot_agent::turn::retention::cancellation_before_effect_keeps_existing_decisions`
`verified-by: bravebot_agent::turn::ordinary_parent_endings_retain_delegate_file_and_program_decisions`
`verified-by: bravebot_tui::undo_tests::ordinary_tui_endings_keep_exact_approvals_advice_and_exposure`

`verified-by: bravebot_agent::turn::an_empty_reply_is_asked_about_and_the_turn_carries_on`
`verified-by: bravebot_agent::turn::two_empty_replies_in_a_row_end_the_turn`
`verified-by: bravebot_agent::turn::completed_empty_reply_keeps_reported_usage_on_failure`
`verified-by: bravebot_agent::backend::only_a_finished_reply_with_nothing_in_it_is_an_empty_reply`

<a id="TURN-7"></a>
### TURN-7: a reply the output ceiling stopped is told why and asked once for less

When a reply reaches its output ceiling part way through a tool call, or before it wrote anything,
the driver adds a line to the conversation naming the ceiling and what the reply was doing, and asks
again. Text the reply wrote before the call is kept as its answer so far. The line says what to do
instead: a file too big for one reply is written in smaller calls with the write tools the request
offered; any other call is made again with less in it; a reply that thought until the ceiling is
asked to act; and a turn with no tools left is asked for a shorter answer. The person is told the
reply stopped and what it is being asked for. A second stop in a row ends the turn: as the failure
naming the ceiling when nothing was written, and with its text when something was, the person told
of any call it was writing that was not made. An answered request in between starts the count
again. A stop wrote something, so it is not an empty reply ([TURN-6](#TURN-6)) and breaks a run of
them.

**The call it was writing is never made.** Its arguments stopped where the ceiling did, so running
it would write half a file or run half a command. The backend returns no call from a reply the
ceiling stopped ([BACKEND-42](backends.md#BACKEND-42)), and the driver drops any that reach it.

**Told why, not sent again unchanged.** A task that asks for one long script has the model spend
the whole ceiling on one `write_file` argument. Sent the same request again it writes the same call
again, and ending the turn there leaves nothing written. Told the size of the ceiling and that the
file can be written in parts, it has a way through.

**A reply cut off in its prose is not asked about.** One that stopped part way through its answer,
with no call open, ends the turn with what it wrote, and the person is told it stopped short. Asked
to carry on, a model tends to start the answer again from the top.

**Once, for the reason an empty reply is asked about once** ([TURN-6](#TURN-6)): a model that runs
out again after being told how to do the work in parts cannot do it in parts here.

**Deciding on it reads nothing untrusted.** Where the reply stopped is the planner's own output
([LABEL-8](labels.md#LABEL-8)). The tool the line names is the request's own copy of the name,
found by matching what the reply was writing against the tools the request offered, so a name the
reply made up is never repeated back to it.

**Every stop is recorded.** The trail gets a line for each: the ceiling, the tool-calling round it
landed on, the offered tool of the call that was open and how many bytes of its arguments had
arrived, whether any reasoning arrived, and whether the turn asked again, kept the text as its
answer, or ended. Counts and the request's own tool name, so the line carries no content
([TRACE-2](trace.md#TRACE-2)). A turn that went on past a stop leaves nothing else saying it
happened, and one that ended on a stop is read back to ask what the ceiling was spent on, which is
what says whether the work wanted splitting.

`verified-by: bravebot_agent::turn::a_reply_cut_off_while_writing_a_call_is_told_so_and_the_turn_carries_on`
`verified-by: bravebot_agent::turn::a_reply_cut_off_after_text_keeps_the_text_and_the_turn_carries_on`
`verified-by: bravebot_agent::turn::two_ceiling_stops_in_a_row_end_the_turn`
`verified-by: bravebot_agent::turn::a_round_between_two_ceiling_stops_starts_the_count_again`
`verified-by: bravebot_agent::turn::a_call_cut_off_at_the_ceiling_is_never_run`
`verified-by: bravebot_agent::turn::a_reply_cut_off_in_its_prose_ends_the_turn_and_says_it_stopped_short`
`verified-by: bravebot_agent::turn::the_line_after_a_ceiling_stop_says_what_to_do_about_it`
`verified-by: bravebot_agent::turn::the_person_is_told_what_a_ceiling_stop_asked_for`
`verified-by: bravebot_agent::turn::a_ceiling_stop_between_two_empty_replies_is_not_two_empty_replies_in_a_row`
`verified-by: bravebot_agent::turn::the_trail_says_what_each_ceiling_stop_was_writing_and_what_the_turn_did`

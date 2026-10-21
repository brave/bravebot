---
id: TURN
title: What bounds a turn
status: normative
governs:
  - crates/agent/src/turn.rs
  - crates/agent/src/conversation.rs
  - crates/agent/src/turn/delegate_end.rs
  - crates/agent/src/turn/setup.rs
  - crates/agent/src/turn/round_steps.rs
  - crates/agent/src/turn/prompts.rs
  - crates/agent/src/turn/task.rs
  - crates/agent/src/turn/sentences.rs
  - crates/agent/src/spend_limit.rs
  - crates/agent/src/repeated_call.rs
documented-by:
  - docs/website/docs/customize/configuration.md
  - docs/website/docs/troubleshooting.md
  - docs/website/docs/reference/cli.md
  - docs/website/docs/using/transcript.md
  - docs/website/docs/reference/tools.md
---

## Scope

How long a turn may go on, what happens when it does not stop, and what is said when it goes on
without producing anything or ends without checking anything. What completed requests cost survives
a later failure or stop. What happens when the model's reply says nothing at all, and when it runs
into its output ceiling. What a session does when it has spent as many tokens as the person said
it may. What a turn does when the planner asks for the same call three times running. How the reads of one
round are run when the planner asks for several together. What the note under a finished turn says
about refusals. What a turn that answers with tasks still open in the planner's list does about
them. What the planner is told when a turn ends in a failure.

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
`verified-by: bravebot_cli::running::a_one_shot_run_is_held_to_the_default_bound`
`verified-by: bravebot_cli::running::a_session_in_lines_is_not_cut_off_at_the_bound_an_unwatched_run_carries`

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

**Plan mode is a turn where no write is possible.** It offers the write tools and refuses every
call to them ([MODE-3](permission-modes.md#MODE-3)), so the line is not said while it is in force.
The mode is read when the round ends, because the person can change it while the turn runs
([MODE-8](permission-modes.md#MODE-8)), and rounds read in plan mode count toward the number once
it has ended. The sandbox mode `read-only` is the same kind of turn: the file tools refuse every
write in it ([SANDBOX-22](sandboxing.md#SANDBOX-22)), so the line is not said. The sandbox mode
does not change during a turn.

**A delegate's write counts.** A worker delegate that changed a file in the turn's workspace has
written: the files are on disk and its report says which, so the line is not said once the delegate
is taken back. Saying it would contradict the work just done and could send the planner to write the
same change twice. It is the same record [TURN-4](#TURN-4) reads.

**Counted since the last answer, not since the turn began.** A stop is usually not the end of a
task: the next prompt is `continue`, and a change the stopped turn asked to write is the change
being continued. An answer usually is the end of one, so the turn after it starts from nothing
written. The count belongs to the conversation, so a session resumed after a stop keeps it. A
fork cut in front of one of the parent's prompts starts from nothing written, because the count
describes the parent's last turns rather than the ones in front of the cut: starting clear costs
at most a nudge the kept turns did not need, where starting set could withhold one they did.

`verified-by: bravebot_agent::turn::a_turn_that_writes_nothing_for_long_enough_is_told_so`
`verified-by: bravebot_agent::turn::a_turn_that_has_written_is_not_told_to_write`
`verified-by: bravebot_agent::turn::a_turn_whose_worker_delegate_wrote_is_not_told_nothing_is_written`
`verified-by: bravebot_agent::turn::a_turn_in_plan_mode_is_not_told_to_write`
`verified-by: bravebot_agent::turn::a_turn_under_read_only_is_not_told_to_write`
`verified-by: bravebot_agent::turn::a_turn_moved_into_plan_mode_is_not_told_to_write`
`verified-by: bravebot_agent::turn::a_turn_moved_out_of_plan_mode_is_told_to_write`
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
builds nothing. A line refused for a credential it left behind counts, since a program in it put
the credential there, and so does a line stopped with the turn, which is stopped only once its
programs have started. A line that ended in any other error does not, since the error does not
say whether a program in it had started. This is the opposite of [TURN-3](#TURN-3)'s nudge and for
the opposite reason:
that line is about what the planner tried to do, and these two are about what the person is
about to act on.

**Work a delegate did.** A delegate working in the turn's own workspace writes and runs there, so
a file it changed and a program it ran count as the turn's from the round the turn takes it back
in, whether it answered, stopped or failed. While one is still working the question waits, since
it may be part way through the build. A delegate in a checkout of its own
([CHECKOUT-7](checkouts.md#CHECKOUT-7)) builds the commit the checkout holds, which has none of
the turn's changes in it, so nothing it does counts. That includes a write it makes in a directory
the person opened beside the working directory, which a checkout leaves reachable, since the
driver's record of a delegate's work does not say where a write landed. A checkout that was given
at least one of the working directory's changes ([CHECKOUT-23](checkouts.md#CHECKOUT-23)) holds the
turn's changes as they were when it was made, so a program the delegate ran there counts, whether or
not every changed path was carried, and what it wrote does not. One that was given none holds the
commit, as a checkout made without changes does, and nothing the delegate does there counts.

`verified-by: bravebot_agent::turn::a_turn_that_writes_without_running_is_asked_about_it`
`verified-by: bravebot_agent::turn::a_turn_that_wrote_and_ran_is_not_asked_about_it`
`verified-by: bravebot_agent::turn::a_write_the_person_refused_is_not_reported_as_a_change_that_was_never_built`
`verified-by: bravebot_agent::turn::a_write_plan_mode_refused_is_not_reported_as_a_change_that_was_never_built`
`verified-by: bravebot_agent::turn::a_run_the_person_refused_leaves_the_change_reported_as_never_built`
`verified-by: bravebot_agent::turn::a_turn_offered_no_run_is_not_reported_as_a_change_that_was_never_built`
`verified-by: bravebot_agent::turn::a_turn_stopped_before_any_write_is_not_told_a_change_was_never_built`
`verified-by: bravebot_agent::turn::a_turn_stopped_after_a_write_is_told_the_change_was_never_built`
`verified-by: bravebot_agent::turn::a_turn_that_failed_after_a_write_is_told_the_change_was_never_built`
`verified-by: bravebot_agent::turn::a_program_a_delegate_ran_in_the_workspace_counts_as_the_turns_own`
`verified-by: bravebot_agent::turn::a_file_a_delegate_changed_in_the_workspace_is_reported_by_the_turn_as_never_built`
`verified-by: bravebot_agent::turn::a_file_a_delegate_changed_before_it_failed_is_reported_by_the_turn_as_never_built`
`verified-by: bravebot_agent::turn::a_program_a_delegate_ran_in_its_own_checkout_leaves_the_change_reported_as_never_built`
`verified-by: bravebot_agent::turn::a_program_a_delegate_ran_in_a_checkout_holding_the_changes_counts_as_the_turns_own`
`verified-by: bravebot_agent::turn::a_program_a_delegate_ran_in_a_checkout_given_none_of_the_changes_leaves_the_change_reported_as_never_built`
`verified-by: bravebot_agent::turn::the_question_waits_for_a_delegate_still_working_in_the_workspace`
`verified-by: bravebot_agent::turn::a_line_refused_for_a_credential_it_left_counts_as_run`
`verified-by: bravebot_agent::turn::a_line_that_failed_before_its_program_started_leaves_the_change_reported_as_never_built`
`verified-by: bravebot_agent::turn::a_line_stopped_with_the_turn_counts_as_run`

<a id="TURN-5"></a>
### TURN-5: completed work remains charged when a turn fails or stops

The turn reports cumulative usage after completed planner, processor and compaction calls, and
when delegates are collected. Each report replaces the previous total. While a delegate runs, each
report of its own adds to the parent's total, so the total includes the requests it has completed
and never drops to the delegate's alone. Collection adds either its successful outcome or its
retained progress on failure, and the total counts that usage once. A failed or stopped parent collects
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
`verified-by: bravebot_agent::shared::what_a_running_delegate_has_spent_is_added_to_the_turns_total_as_it_goes`
`verified-by: bravebot_agent::shared::every_running_delegates_spend_is_in_the_total`
`verified-by: bravebot_agent::shared::a_collected_delegates_spend_is_in_the_total_once`
`verified-by: bravebot_agent::shared::a_delegates_context_reading_is_not_the_turns`
`verified-by: bravebot_agent::shared::a_nested_delegates_spend_reaches_the_turns_total`
`verified-by: bravebot_agent::turn::the_token_total_moves_while_a_delegate_works_and_counts_it_once`
`verified-by: bravebot_agent::turn::a_report_the_policy_refuses_ends_its_delegate_and_leaves_its_spend_in_the_total_once`

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
`verified-by: bravebot_agent::turn::the_person_is_told_nothing_when_an_empty_reply_is_asked_about`

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

<a id="TURN-8"></a>
### TURN-8: a session that has spent its limit asks before the next request

A session may carry a limit, a number of tokens or a number of Leo Premium credits, from the `limit`
setting or from `/limit`. Before a turn sends a request, the driver compares what the session has
spent, in the unit the limit counts, with it. Tokens are what earlier turns were charged, plus what
this turn's completed requests reported ([TURN-5](#TURN-5)). Credits are the credentials the session
has presented, one per premium request ([PREM-5](premium-credentials.md#PREM-5)), those of a delegate
it started included. Once the spent total is at or over the limit, the request is not sent and the person is asked one question
with three answers: stop, go on without a limit, or go on under a new limit, typed in their own
words. A new limit has to be in the unit the limit counts and above what is spent. A figure that is not asks again, up to three
times in all, and then the turn stops.

**Stopping is the default.** A stop, a declined question, an answer that is none of the three, and
an interface that cannot ask all end the turn as a stop. The first row of the question is the stop,
so a bare Enter is the safe answer. A loop ends and a goal stays set, as they do for any stop, so a
`/loop` or `/goal` run ends at the limit and the person is told so.

**No rule and no mode answers it.** The question goes to the person through the same channel as a
question the planner asks, and every permission mode passes it on, bypassing included: a mode that
asks about nothing still stops here, unless the person set no limit. A new limit or going on without
one is the session's from then on, including for a turn already running when `/limit` is typed.

**The figure is a count and the stop depends on no content.** Tokens are the sum of the usage the
backends reported ([BACKEND-31](backends.md#BACKEND-31), [BACKEND-40](backends.md#BACKEND-40)). A
request that reported nothing adds nothing, so a backend that reports no usage is never stopped by
a limit in tokens. Credits are counted as each credential is handed out, whichever request spends
it, so a session that spends none, with no subscription imported, is never stopped by a limit in
credits, and tokens spent do not reach one. The question is built from that count, the limit and fixed words, and reads no reply or tool
result.

**A delegate's own budget is separate.** A delegate is never asked this question. What it spends
reaches the session's total when it is collected, and a token budget of its own
([DELEGATE-29](delegation.md#DELEGATE-29)) stops it by removing its tools, with nobody asked.

**Asked each time.** The question carries a key of its own on every ask, because an interface may
remember an answer by key, and the same figures can come back after a stop.

**Recorded.** The trail gets a line with the round, the spent total, the limit and what the person
chose, which carries no content ([TRACE-2](trace.md#TRACE-2)).

**Only where a person is in front of it.** The terminal session sets the limit on the turns it
starts. A one-shot run, a manifest run and the desktop window set none, for the reason
[TURN-2](#TURN-2) gives, so none of them is asked.

**Known costs.** The check comes before a request, so a limit is passed by the round that reached it
and by whatever a delegate in flight spends before it is collected. A limit counted in tokens says
nothing about money, and is not a per-model price. A limit counted in credits leaves out the
credentials spent by `/compact`, `/aside`, a goal check and a manifest run, which present them
outside the turn; it is not kept when a session is resumed, and `/clear` starts it again from zero.

`verified-by: bravebot_agent::turn::a_turn_under_its_limit_is_not_asked_about`
`verified-by: bravebot_agent::turn::a_turn_that_reaches_its_limit_asks_before_the_next_request_and_stops_on_a_stop`
`verified-by: bravebot_agent::turn::a_session_already_past_its_limit_is_asked_before_the_first_request`
`verified-by: bravebot_agent::turn::what_the_session_spent_before_the_turn_counts_towards_the_limit`
`verified-by: bravebot_agent::turn::a_new_limit_typed_at_the_question_lets_the_turn_go_on_under_it`
`verified-by: bravebot_agent::turn::a_figure_that_is_not_above_what_was_spent_is_asked_about_again`
`verified-by: bravebot_agent::turn::unusable_figures_three_times_in_a_row_stop_the_turn`
`verified-by: bravebot_agent::turn::going_on_without_a_limit_clears_it`
`verified-by: bravebot_agent::turn::a_question_nobody_answers_stops_the_turn`
`verified-by: bravebot_agent::turn::no_permission_mode_answers_the_limit_question`
`verified-by: bravebot_agent::turn::a_limit_in_credits_asks_once_that_many_credentials_are_spent`
`verified-by: bravebot_agent::turn::tokens_spent_do_not_reach_a_limit_in_credits`
`verified-by: bravebot_agent::turn::credits_spent_do_not_reach_a_limit_in_tokens`
`verified-by: bravebot_agent::turn::a_figure_in_the_other_unit_is_not_a_limit`
`verified-by: bravebot_agent::turn::a_credential_a_delegate_spends_is_counted`
`verified-by: bravebot_agent::turn::two_questions_about_the_same_figures_are_different_questions`
`verified-by: bravebot_agent::turn::a_session_with_no_limit_is_never_asked`
`verified-by: bravebot_tui::app::a_turn_shares_the_sessions_limit_and_carries_what_was_spent`
`verified-by: bravebot_config::settings::only_a_positive_count_is_a_session_limit`

<a id="TURN-9"></a>
### TURN-9: the same call three times running is held and asked about

When the planner asks for a call whose tool name and arguments equal those of the two calls
before it, with no different call between them, the driver does not run it. The person is asked
one question with three answers: do not run it and tell the planner to try another approach, run
it this once, or stop the turn. A refusal is answered to the planner in place of a result, as the
driver's own error that names the tool and the count. Each further identical call is held in the
same way.

**Arguments only.** The comparison reads the tool name and the arguments the planner wrote, with
the keys of a JSON object in sorted order, so the same object written in another key order is the
same call. It never reads a result. A result can carry untrusted bytes, which the driver may not
branch on, and the planner's own call is built from a context holding none ([LABEL-8](labels.md#LABEL-8)).

**What it costs.** A call whose result changes between identical calls, such as polling
`read_output` or running a test again without an edit between, is held at the third. A different
call between two identical ones starts the count again, so a write between two runs does.

**A yes covers one call.** It runs that call and starts the count again, so the next held call is
the third after it. The question carries a key of its own on every ask, because an interface may
remember an answer by key.

**No rule and no mode answers it.** Every permission mode passes the question on, bypassing
included, for the reason [TURN-8](#TURN-8) gives. Where nobody can be asked, which is a one-shot
run, a manifest run, a closed channel or a delegate, the call is dropped and the planner told
([PROMPT-9](prompting.md#PROMPT-9)). The first row is the refusal, so a bare Enter is the safe
answer, and an answer that is none of the three is a refusal.

**Not a safety property,** for the reason [TURN-1](#TURN-1) gives. A call that differs by one
argument each round is not caught, such as the glob after glob that clause describes.

**Recorded.** The trail gets a line with the round, the count and the answer, which carries no
call and no result ([TRACE-2](trace.md#TRACE-2)). There is no setting for the number.

`verified-by: bravebot_agent::turn::a_third_identical_call_is_not_run_and_the_person_is_asked`
`verified-by: bravebot_agent::turn::a_differing_argument_starts_the_count_again`
`verified-by: bravebot_agent::turn::a_yes_runs_the_call_once_and_the_count_starts_again`
`verified-by: bravebot_agent::turn::a_stop_at_the_question_ends_the_turn`
`verified-by: bravebot_agent::turn::where_nobody_can_be_asked_the_call_is_dropped_and_the_planner_told`
`verified-by: bravebot_agent::turn::no_permission_mode_answers_the_repeated_call_question`
`verified-by: bravebot_agent::turn::the_trail_records_the_count_and_the_answer_and_nothing_the_call_carried`
`verified-by: bravebot_agent::turn::two_held_calls_are_two_questions`
`verified-by: bravebot_agent::repeated_call::the_same_arguments_in_another_key_order_are_the_same_call`

<a id="TURN-10"></a>
### TURN-10: the independent reads of a round run at the same time

When one reply from the planner asks for several calls, each run of consecutive `list_files` and
`search` calls is read at the same time, at most 8 at once. Every other call runs alone and in the
order it was asked, after the calls before it have finished and before the calls after it start.
`read_file`, `read_git` and `lsp` are among them. Each can ask the person, call a model, or keep
state that a later call reads, so running one beside another call would change what that call sees
or the order the person is asked in.

**Nothing the planner is given changes.** Results are added in the order the calls were asked and
not the order the reads finished. A reference is numbered, a note drawn and a line recorded in the
trail where that call's turn comes, so a round of reads leaves the same conversation, trail and
screen as the same calls run one after another. A call in a run is announced when its result is
taken and not while it reads.

**What joins a run.** A call joins when it is a `list_files` or `search` that this run offers, its
arguments are JSON, the person has declared no hook for a finished call of that tool, and it would
not be held as a third identical repeat ([TURN-9](#TURN-9)). One such call is not a run. A hook is
left out because it can change the files the next read looks at, and a held call asks a question.

**Each read decides as the turn would.** A read runs under a branch of the turn's policy, which
holds the same rules, capabilities and context, shares the one file authority, and records into a
buffer of its own. The buffer reaches the turn's trail when that call's turn comes, and a refusal in
it is counted then. A read is used only while the planner's context is what it was when the run
began. If the context has met something untrusted since, the call runs for itself under the context
the calls before it left. No label, routing or approval decision changes.

**Reads wait for effects and not for each other.** A read holds the file authority's access lock
shared, and an effect entering or completing holds it exclusive ([TRUST-4](trust-map.md#TRUST-4)).
Two reads therefore overlap, and neither captures bytes while an effect on a file is entering.

**Cancelling.** The cancel is checked before the run starts, before a thread starts each read, and
before each call is taken. A read already running finishes, as a search does when it runs alone. A
call after the one the cancel stopped at never has its result used or its lines recorded, so the
trail holds the calls before the cancel and no others.

**No setting.** The limit of 8 and what joins a run are fixed.

**Known costs.** A run that is slow shows nothing until its first call is announced, where a
call run alone is announced as it starts. The time a run took is added to the turn's tool time once,
and each call in it is shown as finishing at once.

`verified-by: bravebot_agent::turn::results_come_back_in_the_order_the_calls_were_made`
`verified-by: bravebot_agent::turn::references_are_numbered_in_the_order_the_calls_were_made`
`verified-by: bravebot_agent::turn::a_call_that_writes_separates_the_reads_before_it_from_the_reads_after`
`verified-by: bravebot_agent::turn::a_cancel_during_the_reads_leaves_only_the_calls_before_it_in_the_trail`
`verified-by: bravebot_agent::turn::a_run_of_reads_ends_at_a_call_that_asks_or_is_hooked_or_would_be_held`
`verified-by: bravebot_agent::tools::reads_taken_beforehand_leave_the_trail_the_screen_and_the_answers_a_sequential_run_does`
`verified-by: bravebot_agent::tools::the_reads_of_a_round_are_inside_their_captures_at_the_same_time`
`verified-by: bravebot_agent::tools::no_more_reads_run_at_once_than_the_limit`
`verified-by: bravebot_agent::tools::a_cancel_stops_the_reads_not_yet_started`
`verified-by: bravebot_agent::tools::reads_taken_beforehand_wait_for_an_effect_entering_a_file`
`verified-by: bravebot_agent::tools::a_read_is_used_only_while_the_context_is_what_it_ran_under`
`verified-by: bravebot_agent::tools::only_a_read_the_run_offers_is_taken_beforehand`
`verified-by: bravebot_core::policy::a_branch_decides_what_the_policy_it_came_from_would`
`verified-by: bravebot_core::policy::a_branch_reads_the_file_decisions_made_after_it_began`
`verified-by: bravebot_core::policy::what_a_branch_records_reaches_the_policy_when_it_rejoins_and_in_order`

<a id="TURN-11"></a>
### TURN-11: the note under a finished turn names only the refusals no later call got past

A turn that ends with a gate having refused a call says so under the reply, as a note. A refusal
is *standing* unless a later call to the same tool was refused by no gate, which recovers every
refusal earlier calls to that tool left standing. A recovered refusal is not drawn. A turn whose
refusals are all recovered shows no note, and the turn is still not clean: the refusals stay in
the audit trail ([TRACE-1](trace.md#TRACE-1)) and are counted when the turn finishes.

**One tool is one purpose, as far as this goes.** A later `run` of another line recovers an
earlier refused `run`. The decision only picks what is drawn, so a wrong guess hides nothing the
trail does not hold. A refusal a branch recorded and the turn took back at rejoin is never
recovered, because no call to a tool settled it.

**Counted, and the first named.** Where refusals stand the note gives how many and names the
first, as `gate: reason`. It does not list the rest. The general sentence that a gate refused
something is said only where the turn was not clean and holds no list of what was refused, as a
manifest run's does not.

**No result is read.** Which tool a call named and whether a gate refused it are the only facts
used, so nothing untrusted a tool returned reaches the decision ([LABEL-8](labels.md#LABEL-8)).

`verified-by: bravebot_core::policy::a_refusal_a_later_call_to_the_same_tool_got_past_is_not_standing`
`verified-by: bravebot_core::policy::a_refusal_stands_until_the_same_tool_runs_without_one`
`verified-by: bravebot_core::policy::a_rejoined_refusal_stands_whatever_runs_after_it`
`verified-by: bravebot_agent::turn::a_refusal_a_later_call_to_the_same_tool_got_past_is_not_left_standing`
`verified-by: bravebot_agent::turn::a_refusal_no_later_call_to_its_tool_got_past_stays_standing`
`verified-by: bravebot_tui::app::a_refusal_a_later_call_got_past_is_not_drawn_and_one_left_standing_is_counted_and_named`

<a id="TURN-12"></a>
### TURN-12: a turn that answers with tasks still open asks about the list once

Where `todo_write` is offered, the latest accepted list leaves at least one task pending or in
progress, and the planner answers without calling a tool, the driver puts that answer into the
conversation and says how many tasks are open. It asks for the whole list again, with a task marked
completed where it is done and cancelled where it was dropped, and says that where work remains the
tasks stay open and the answer says what is left. The turn carries on with its tools and ends on
the planner's next answer, whatever the list says then. The line is a count and nothing about the
tasks.

**The list is the one the panel keeps.** The plan stays after the turn that wrote it
([PANEL-10](info-panel.md#PANEL-10)), so the question is about the session's last accepted list
whichever turn wrote it, and a turn that wrote none is asked about the one before. A list the
planner was not shown, as in a session that has met something untrusted, ends the question about
the list before it, because the count of that one no longer describes the panel's. The count is
kept for a list whichever way its result went back to the planner. Pending and in progress are open,
finished and dropped are not, and a status the driver does not recognise reads as pending
([TODO-2](tools/todo-write.md#TODO-2)).

**Asked once for each list.** A list the planner writes after the question, in the turn that put
it, is its answer to the question and is not asked about, whatever it holds. A planner that
answers with the list as it was, or sends it again with a task still open, is not asked again, in
that turn or in the turns after it. A list written in a later turn is a new list. A task may be open
for a reason the driver cannot read, such as a question the planner put to the person, and a request
spent each turn on a list nothing in that turn touched would be the cost of asking every time. The
count is read once, whether or not it leads to the question, so a list with nothing open is not read
again at the end of every turn after it.

**Conditional in its wording, not in its firing.** The driver cannot tell a list the planner
finished from one it left open on purpose, and must not try. It knows how many tasks are open, and
the planner knows whether that is wrong, as with [TURN-3](#TURN-3). The line also asks the planner
to give its answer again in full, since the answer after the list is the one shown as the reply and
the first is drawn between the calls.

**Read only where the list is trusted.** Whether to ask is a decision. The kernel counts the open
tasks where it can read the statuses and labels the count as the list is labelled, and the driver
reads it through `Policy::read_trusted_content` ([LABEL-6](labels.md#LABEL-6)), which refuses a
count that is not trusted. In a session that has met something untrusted the list is labelled
untrusted and the planner is shown none of it, so no count is kept and the turn ends on the answer.
No task's text reaches the planner through this line.

**Not asked where it cannot be acted on.** A turn that is not offered `todo_write`, such as one
addressed to a definition that leaves it out, cannot send a list, and the question waits for a turn
that can. A turn whose next request offers no tools ([TURN-1](#TURN-1)) ends on the answer it has.
A reply the output ceiling stopped in its prose is the turn's answer as it stands
([TURN-7](#TURN-7)). A turn the person stopped as the answer arrived is not asked anything further,
since the next request would end it as stopped.

**What the person typed.** A line the person typed as the answer arrived is put in front of the
planner with the question, as the line typed while a delegate runs is put in front of the round after
its report.

**Known costs.** The count is not written to the session record, so a session resumed with a list
still open is not asked about it, though the panel shows that plan
([PANEL-10](info-panel.md#PANEL-10)). The first turn that answers ends on its answer, and the
question comes only for a list written after the resume, as [COMPACT-16](compaction.md#COMPACT-16)
carries a list forward only from the next `todo_write`.

`verified-by: bravebot_agent::turn::a_turn_that_answers_with_tasks_still_open_is_asked_to_bring_the_list_up_to_date`
`verified-by: bravebot_agent::turn::a_turn_asks_about_open_tasks_once_however_the_planner_answers`
`verified-by: bravebot_agent::turn::a_planner_that_answers_the_question_with_another_open_list_is_not_asked_again`
`verified-by: bravebot_agent::turn::a_list_with_every_task_completed_or_cancelled_is_not_asked_about`
`verified-by: bravebot_agent::turn::a_list_left_open_by_an_earlier_turn_is_asked_about_when_a_later_turn_answers`
`verified-by: bravebot_agent::turn::a_list_the_planner_was_asked_about_is_not_asked_about_by_the_next_turn`
`verified-by: bravebot_agent::turn::a_list_written_after_the_planner_was_asked_is_asked_about_afresh`
`verified-by: bravebot_agent::turn::a_list_the_planner_sends_again_when_asked_is_not_asked_about_by_the_next_turn`
`verified-by: bravebot_agent::turn::a_list_the_planner_was_not_shown_ends_the_question_about_the_list_before_it`
`verified-by: bravebot_agent::turn::a_list_written_in_a_round_with_no_call_ids_is_asked_about`
`verified-by: bravebot_agent::turn::a_line_typed_as_the_planner_answers_reaches_it_with_the_question_about_the_list`
`verified-by: bravebot_agent::turn::a_reply_cut_off_in_its_prose_is_not_asked_about_open_tasks`
`verified-by: bravebot_agent::turn::a_session_that_has_met_something_untrusted_is_not_asked_about_the_list`
`verified-by: bravebot_agent::turn::a_turn_not_offered_the_task_list_tool_is_not_asked_about_the_list`
`verified-by: bravebot_agent::turn::a_turn_stopped_as_it_answers_is_not_asked_about_open_tasks`
`verified-by: bravebot_agent::tools::an_accepted_list_carries_the_count_of_its_open_tasks_at_its_own_label`
`verified-by: bravebot_agent::tools::the_count_of_open_tasks_in_an_untrusted_list_is_refused_to_the_driver`
`verified-by: bravebot_agent::tools::a_refused_call_carries_no_count_of_open_tasks`
`verified-by: bravebot_core::todo::a_task_is_open_unless_it_is_finished_or_dropped`

<a id="TURN-13"></a>
### TURN-13: a turn that ends in a failure leaves the planner a note that it did

When a turn ends as a failure ([TURN-5](#TURN-5)) after it has added messages to the conversation,
the driver adds one more after them, so the next prompt follows it. The note says that the turn
failed and ended before it finished, that it did not pause, and that nothing is continuing it. It
gives the failure's category and, where it is known, the number of requests sent for the call that
failed. A turn that answered, a turn that failed before it added anything to the conversation, and
a turn the person stopped add no note. A stop is the person's own pause, and the next prompt usually
continues the turn ([TURN-3](#TURN-3)).

**The words are the driver's.** They are fixed, and the category and the request count are fields
of the diagnosis ([BACKEND-37](backends.md#BACKEND-37)). The category is a fixed name and the count
is the driver's own, so nothing the service sent is in the note: not its reply, its headers, its
URL, its error text or the status of its reply. The category is what the driver made of that
status, and it is all the planner is given of any other failure ([BACKEND-37](backends.md#BACKEND-37)
says the same of a processor's). A background session started again after an interrupted turn is
told in the same way, in the driver's own words, that the turn ended without finishing
([BG-12](background-sessions.md#BG-12)).

**It is part of the saved conversation.** The note is a notice. It is saved with the conversation,
so a session resumed after a failed turn holds it as the live session did. It is drawn as nothing
anyone said, and it is not a prompt a fork can be cut in front of or a point a cut between
exchanges falls in front of. The note a resume adds about dead references is left out of the record
because each resume adds its own. This note describes one turn, so a resume that left it out would
again give the planner tool results with no ending.

**Every front end that keeps a conversation has it.** The note is added where the turn ends, not
where an interface records the failure ([SESSION-23](sessions.md#SESSION-23)), so the terminal, the
line-mode session and the desktop window carry it into the next prompt. A one-shot run that fails
writes no record, so no later prompt follows its conversation.

**Why.** A turn that fails after some tool calls leaves those calls and their results in the
conversation. Nothing in the conversation says they stop because of a failure, so a planner asked
"what happened?" cannot tell a failed turn from one it paused itself, and can answer that it will
carry on. The failure is recorded for the person alone. A turn that added nothing leaves nothing
for a note to refer to.

**Known costs.** Each failed turn adds its own note and a later turn that succeeds does not remove
it, so a session that fails repeatedly holds one for each failure. A note is about fifty tokens.

`verified-by: bravebot_agent::turn::a_turn_that_ends_on_a_failed_request_leaves_the_planner_a_note_of_it`
`verified-by: bravebot_agent::turn::a_turn_that_failed_before_adding_anything_leaves_no_note_and_one_that_added_its_prompt_does`
`verified-by: bravebot_agent::turn::a_turn_that_answered_or_was_stopped_leaves_no_note_of_a_failure`
`verified-by: bravebot_agent::turn::a_turn_that_compacted_and_then_failed_leaves_the_note_of_the_failure`
`verified-by: bravebot_agent::conversation::the_note_about_a_failed_turn_is_sent_and_kept_and_is_not_drawn`
`verified-by: bravebot_agent::conversation::the_note_about_a_failed_turn_holds_the_category_and_request_count_and_nothing_else`
`verified-by: bravebot_tui::sessions::a_failed_turn_leaves_a_note_the_transcript_does_not_draw_and_a_resume_still_sends`
`verified-by: bravebot_ui_bridge::history::a_failed_turn_leaves_the_planner_a_note_in_the_saved_conversation`
`verified-by: bravebot_cli::running::a_session_in_lines_tells_the_planner_that_the_turn_before_it_failed`

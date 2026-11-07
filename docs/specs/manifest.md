---
id: MANIFEST
title: Plan then execute
status: normative
governs:
  - crates/core/src/manifest.rs
  - crates/core/src/apply.rs
  - crates/choice/src/rows.rs
  - crates/core/src/policy.rs
  - crates/agent/src/manifest.rs
  - crates/agent/src/connectors.rs
  - crates/agent/src/server_calls.rs
  - crates/agent/src/mode.rs
  - crates/cli/src/main.rs
  - crates/cli/src/invocation.rs
  - crates/cli/src/flags.rs
  - crates/cli/src/finish.rs
  - crates/cli/src/one_shot.rs
  - crates/session/src/sessions.rs
  - crates/tui/src/resume.rs
  - crates/tui/src/confirm.rs
  - crates/tui/src/confirm/grants.rs
  - crates/tui/src/confirm/pinned.rs
  - crates/tui/src/app.rs
  - crates/tui/src/app/turns.rs
  - crates/tui/src/app/actions.rs
  - crates/tui/src/manifest_run.rs
  - crates/ui-bridge/src/manifest.rs
guards:
  - symbol: Policy::before_planning
  - symbol: Policy::adopt_manifest
  - symbol: Policy::quarantine
  - symbol: Manifest::routing
documented-by:
  - docs/website/docs/using/headless.md
  - docs/website/docs/using/sessions.md
  - docs/website/docs/using/manifest-mode.md
---

## Scope

The other way a task is run. A turn decides what to do next after each thing it reads. A manifest
run decides everything first: the planner emits a step list, the kernel refuses it or freezes it,
and a driver walks it with no model in the control path.

The default remains the turn loop. This mode is not a stricter policy. The gates are the same;
what changes is the scope of the precommitment, from one turn to a whole run.

## Clauses

<a id="MANIFEST-1"></a>
### MANIFEST-1: planning is gated, not counted

A plan may be asked for only from a planner whose context holds the task string, the pictures a
person pasted into it or dropped onto it, the driver's own words, and the names on a list of a
server's tools that a person vouched for ([MANIFEST-15](#MANIFEST-15)), and has been shown nothing
else. A paste is a keystroke of the person's own and says nothing about content anybody else wrote
([pasting.md](pasting.md#PASTE-5)), which is why a screenshot of the thing to be built goes in with
the words it was pasted beside while a pipe is still refused (MANIFEST-9). A drop is the same
gesture reaching a file rather than a clipboard: the path is fixed before any request goes out and
the file is read outside this planner's policy, which holds no read at all
([dropping.md](dropping.md#DROP-4)), so the plan is still fixed before anything the plan itself
could look at. Everything else is already excluded everywhere, since untrusted content is never
shown to the planner. The gate is that invariant written where a change has to pass it.

Two calls happen: the goal in plain words, then the same work fitted to the tool set. What is
forbidden is a re-plan: a plan that fails validation fails the run, and nothing plans again once
a step has read something. The one exception comes before anything is read or put to anybody: a
plan whose connector steps fail the check against their definitions, or whose server calls fail
the check against a vouched list ([MANIFEST-15](#MANIFEST-15)), is asked for once more, the
fit call being shown the plan it gave and the check's own words about it, and a second plan that
fails anything fails the run with its own refusal. What that call is shown is the planner's own
plan and the driver's words, so its context is still the one this clause admits, and a plan with
no connector step and no server call never fails that check, so a run that uses neither is never
asked again.
Where no connector is usable, a connector step names a capability the catalogue did not offer, and
the plan is refused as one naming any other unknown capability is, without that check.

`verified-by: bravebot_core::policy::every_planning_call_is_allowed_from_a_clean_context`
`verified-by: bravebot_core::policy::a_planner_shown_untrusted_content_may_not_plan`
`verified-by: bravebot_agent::manifest::the_audit_trail_records_each_planning_call`
`verified-by: bravebot_agent::manifest::a_manifest_that_is_not_json_fails_without_another_call`
`verified-by: bravebot_agent::manifest::a_plan_refused_by_a_connector_check_is_asked_for_once_more`
`verified-by: bravebot_agent::manifest::a_connector_step_where_no_connector_is_usable_is_not_a_capability`
`verified-by: bravebot_agent::manifest::a_plan_refused_by_a_connector_check_twice_fails_with_the_second_refusal`
`verified-by: bravebot_agent::manifest::a_plan_that_fails_validation_is_not_asked_for_again_where_a_connector_is_installed`
`verified-by: bravebot_agent::manifest::a_picture_pasted_into_the_task_reaches_the_planner`
`verified-by: bravebot_agent::manifest::a_picture_dropped_onto_the_task_reaches_the_planner`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_the_list_refuses_is_asked_for_once_more`

<a id="MANIFEST-2"></a>
### MANIFEST-2: a plan that is not trusted is refused

A plan is a program, so every field in it is a decision. A plan derived from anything an attacker
wrote is an attacker choosing the steps, and there is no repair for that.

`verified-by: bravebot_core::policy::a_plan_from_a_clean_context_is_adopted`
`verified-by: bravebot_core::policy::a_plan_from_a_tainted_context_is_refused`
`verified-by: bravebot_core::policy::a_tainted_planner_is_refused_at_the_call_and_at_adoption`

<a id="MANIFEST-3"></a>
### MANIFEST-3: a run that stopped comes back with what it produced

The plain-words goal, the proposed manifest verbatim, the frozen plan, and what each step did.
On success they travel with the outcome; on failure they travel with the error. A plan that would
not parse has no rendered form, so the model's own words are the only thing left to look at.

Never drop an artefact on the error path, and never make inspecting one conditional on a flag.

`verified-by: bravebot_agent::manifest::a_manifest_that_will_not_parse_comes_back_verbatim`
`verified-by: bravebot_agent::manifest::a_proposal_is_kept_including_its_packaging`
`verified-by: bravebot_agent::manifest::a_plan_that_fails_the_schema_keeps_the_goal_and_the_proposal`
`verified-by: bravebot_agent::manifest::a_step_that_fails_leaves_the_plan_and_everything_that_ran`
`verified-by: bravebot_cli::main::a_failed_plan_is_printed_beside_the_reply`
`verified-by: bravebot_cli::main::a_stopped_manifest_prints_what_it_produced_without_the_trace_flag`

<a id="MANIFEST-4"></a>
### MANIFEST-4: validation is pure and total

The validator sees a draft and nothing else. Any violation fails the run whole. A manifest is
never half adopted, and a step is never repaired to make a plan usable.

`verified-by: bravebot_core::manifest::an_empty_plan_is_refused`
`verified-by: bravebot_core::manifest::a_tool_outside_the_schema_is_refused`
`verified-by: bravebot_core::manifest::a_path_outside_the_workspace_is_refused`
`verified-by: bravebot_core::manifest::a_read_after_an_action_is_refused`
`verified-by: bravebot_core::manifest::validation_is_deterministic`
`verified-by: bravebot_core::manifest::a_body_field_must_hold_text`
`verified-by: bravebot_agent::manifest::a_plan_that_fails_validation_runs_nothing`
`verified-by: bravebot_agent::manifest::a_plan_whose_body_field_is_not_text_is_refused`

<a id="MANIFEST-5"></a>
### MANIFEST-5: the driver adds nothing

It may not insert, skip, reorder, or synthesise a step, and there is no handler for a tool the
schema does not name. If a plan cannot express something, the plan is wrong, not the driver.

`edit_file` is unavailable, because locating a passage means having read the file and the planner
has read nothing. `todo_write` is unavailable because the manifest is already the task list.
There is no shell and no `run`: a command string is destination and payload at once. The one
program this mode starts is a connector's authority, at the absolute path in the person's own
definition, with one fixed argument and the request on its standard input
([connectors.md](connectors.md)).

A tool of an MCP server is not a tool of this schema. It is the `tool` field of an `mcp_call` or
`mcp_read` step, whose handlers call only a tool on a list a person vouched for, so the schema still
names every handler ([MANIFEST-15](#MANIFEST-15)).

`verified-by: bravebot_agent::manifest::a_connector_fetch_fills_one_slot_that_no_planner_reads`
`verified-by: bravebot_agent::manifest::a_connector_fetch_fills_one_unread_slot_and_the_act_sends_the_released_text`
`verified-by: bravebot_agent::manifest::injected_text_in_a_file_cannot_add_a_step`
`verified-by: bravebot_agent::manifest::the_planner_is_shown_capabilities_and_no_tool_names`
`verified-by: bravebot_agent::manifest::the_registry_and_the_kernel_schema_describe_the_same_tools`
`verified-by: bravebot_core::manifest::no_action_in_the_schema_fills_a_slot`
`verified-by: bravebot_agent::manifest_server_calls::a_call_the_schema_or_the_list_cannot_hold_runs_nothing`

<a id="MANIFEST-6"></a>
### MANIFEST-6: routing is locked from the manifest before the first step

Every destination the plan will use is inserted through the gate that refuses anything not
trusted and public, before a single byte has been read. Steps read their destinations out of
that lock, not out of themselves.

A destination is text. Validation establishes that for every routing field, so a field the plan
filled with a number or a list refuses the plan whole rather than reaching the lock.

Optional routing is filled at validation with an explicit default, where the plan omitted the
field or left it empty and nowhere else, so the driver never invents a destination that did not
pass the lock and a default never stands in for a value the plan gave. An omitted listing pattern
or search include is locked as no filter, not as a glob of the empty string, which would match
nothing. A connector step's connector, resource, place, item, selector and `with` values, and an
act's path, option and `take`, are routing, and the optional ones are locked as the absence of a
value. The item a
connector act lands on, where the plan did not name it, is named by `on`: a slot reference to the
fetch that chose it, not a routing field. That fetch has to be of the same connector and place as
the act, or the plan is refused, so an act cannot reach an item from anywhere its own locked place
does not cover.

A search step prints each hit as `path:line:text` and each line of context as `path-line-text`, with
the text following the separator byte for byte, as the search tool does
([SEARCH-10](tools/search.md#SEARCH-10)).

`verified-by: bravebot_core::manifest::every_effect_destination_is_named_for_the_routing_lock`
`verified-by: bravebot_core::manifest::an_omitted_search_directory_is_locked_as_the_workspace`
`verified-by: bravebot_core::manifest::an_omitted_list_pattern_is_locked_as_no_filter`
`verified-by: bravebot_core::manifest::a_pattern_the_plan_left_empty_is_locked_as_no_filter`
`verified-by: bravebot_core::manifest::a_routing_field_that_is_not_text_is_refused`
`verified-by: bravebot_core::manifest::a_routing_default_never_replaces_a_field_the_plan_gave`
`verified-by: bravebot_agent::manifest::a_plan_whose_routing_field_is_not_text_is_refused`
`verified-by: bravebot_agent::manifest::a_write_lands_where_the_plan_said_and_carries_what_it_never_read`
`verified-by: bravebot_agent::manifest::a_listing_with_no_pattern_lists_the_tree`
`verified-by: bravebot_agent::manifest::a_search_step_prints_the_line_unchanged_after_path_and_line_number`
`verified-by: bravebot_core::manifest::a_connector_fetch_locks_every_routing_field_and_an_omitted_one_as_no_value`
`verified-by: bravebot_agent::connectors::a_default_never_replaces_a_value_the_plan_gave_in_another_shape`
`verified-by: bravebot_core::manifest::a_connector_plan_locks_every_routing_field_and_not_its_target_slot`
`verified-by: bravebot_core::manifest::a_connector_act_naming_its_item_validates_without_a_fetch`
`verified-by: bravebot_agent::manifest::an_act_naming_a_fetch_from_another_place_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::manifest::an_act_on_a_fetch_that_returned_no_receipt_is_never_sent`

<a id="MANIFEST-7"></a>
### MANIFEST-7: the only path to a screen is the one the plan named

The slots a person may be shown are named from the manifest before the policy exists. Nothing
widens that set, and nothing releases a slot the plan did not name.

`verified-by: bravebot_core::manifest::only_the_answered_slot_is_named_for_release`
`verified-by: bravebot_agent::manifest::only_the_answered_slot_reaches_the_user`

<a id="MANIFEST-8"></a>
### MANIFEST-8: everything a step produces is quarantined

There is no planner left to show anything to. A step result is stored, whatever its label, and
the only ways out of a slot are a later processor, a write back into the workspace, and a
release the plan named in advance.

An unmarked processor answer is a remark, not a document, except for a slot an answer reads and
nothing else does. Treating it as the slot would write an explanation over whatever the plan named
next. Such an answer is asked for once more, with the plan's instruction and one fixed sentence,
both trusted, and a second answer without the mark fails the step. An empty reply is asked for once
more as it stands, sharing that one further try. A slot only shown to the person has no file for a
remark to land on, and what the processor said reaches the person either way, so there it stands as
the answer.

`verified-by: bravebot_core::policy::a_step_result_is_quarantined_whatever_its_label`
`verified-by: bravebot_agent::manifest::an_unmarked_transform_does_not_become_a_file`
`verified-by: bravebot_agent::manifest::an_unmarked_transform_is_asked_once_more_for_its_document`
`verified-by: bravebot_agent::manifest::an_empty_processor_reply_is_asked_for_once_more`
`verified-by: bravebot_agent::manifest::a_processor_gets_one_further_try_whatever_went_wrong`
`verified-by: bravebot_agent::manifest::an_unmarked_answer_for_a_slot_only_shown_stands_as_the_answer`
`verified-by: bravebot_agent::manifest::an_unmarked_answer_for_a_slot_a_write_also_reads_still_fails`
`verified-by: bravebot_agent::manifest::a_slot_no_step_reads_is_never_opened`

<a id="MANIFEST-9"></a>
### MANIFEST-9: the default is the turn loop

An unqualified run, and every session, is still observe-decide-act. Manifest is an opt-in: a mode
named on the command line, or a command typed in a session, which starts one run and gives the
session back afterwards (MANIFEST-11). What a session may not do is hold the mode. A session is
several turns over one conversation and each of them decides what to do next after the last one
read something, so a session that was in this mode would be a contradiction; one run started from
one is not.

An unknown mode name is refused rather than guessed, because guessing wrong here would silently run
the mode the user did not ask for.

Piped stdin is refused rather than dropped. A pipe is observed context, and this mode does not
observe before it plans. Name a workspace file instead.

`verified-by: bravebot_agent::mode::the_default_is_the_turn_loop`
`verified-by: bravebot_agent::mode::an_unknown_mode_is_refused`
`verified-by: bravebot_cli::main::the_default_mode_is_the_turn_loop`
`verified-by: bravebot_cli::main::an_unknown_mode_is_refused_rather_than_guessed`
`verified-by: bravebot_cli::main::a_leading_mode_flag_is_a_task_not_an_unknown_option`
`verified-by: bravebot_agent::manifest::piped_input_is_refused_rather_than_dropped`
`verified-by: bravebot_tui::app::a_session_can_ask_for_a_manifest_run`
`verified-by: bravebot_tui::app::a_manifest_run_needs_a_task_to_plan`

<a id="MANIFEST-10"></a>
### MANIFEST-10: a plan runs only once somebody has said yes to it

Between freezing a plan and walking it, the frozen plan is put to a person: the task in their own
words, then every step in order, each naming its tier, what it would do, and every routing field the
step fixed. The routing is part of what is being answered for, so the line a person reads carries the
directory a search runs in and the glob it filters by, not the pattern alone. A connector act's line
also names where the content it carries came from and where it goes (MANIFEST-14), since approving
the plan is what releases it, and the text and title the plan wrote for it, the first 200 characters
of each on one line with control characters escaped. A server call's line names the server, the
tool and the arguments in full. A step that names a file draws the name of that file the way every
other prompt does ([PROMPT-1](prompting.md#PROMPT-1)), so `note.md ` with a space at its end is
quoted and does not read as `note.md`. Which fields name a file is the tool's contract, never the
value: the `path` of a read or a write and the `directory` of a listing or a search. The summary, the
question and the record show a step through one function and draw it alike. A file write that
carries a slot names where
that slot's content came from, in the same words, wherever that is anything but the workspace.
Declining stops the run, and what was declined comes back with the
error under MANIFEST-3, so the plan can still be read.
Nothing has been read or written by then, so a declined run leaves the workspace exactly as it was.

The question is asked once, and there is no standing form of it. Nothing between the plan and the
run can reshape the plan, so there is nothing to ask a second time; and a plan is written afresh for
each run, so remembering an answer would be approving steps nobody has seen.

Approving a plan is not approving its writes. This mode widens the scope of the precommitment and
does not replace the gates inside it, so each write is still put to the person as its step reaches
it. A call to a server's tool is the exception: the line approved shows the call and every argument
the step will send, so approving the plan answers the question the call would put, and nothing else
that decides it ([MANIFEST-15](#MANIFEST-15)).
A connector act is sent to the authority in every mode that runs the plan, bypass included, and its
request carries no answer: the driver asks nothing about the act, and the person is asked in the
authority's popup. Plan mode runs no such plan ([permission-modes.md](permission-modes.md)).

Where nobody can be asked the answer is no, as it is everywhere else. A command typed at a terminal
is somebody: the plan goes out beside the progress and the answer is read back, which is the one
question a one-shot answers ([cli.md](cli.md#CLI-8)). Piped or redirected there is nobody, and the
run stops before its first step unless permissions were skipped outright.

Bypassing approves it, and that is not the standing form ruled out above. A standing answer would
outlive the run that gave it and cover a plan written afterwards; this is a mode somebody typed for
this run, recorded nowhere, and the next run asks again unless the flag is given again
([permission-modes.md](permission-modes.md#MODE-10)). What the flag costs is its own entry there.

**Why.** The gates the other clauses install are all about what a plan may say. None of them is
about whether anybody wanted it: a validated, wholly untainted plan is still a model's choice of a
run's worth of effects, and this mode's own guarantee, that nothing after the plan can reshape it, is
what makes one question about the whole of it worth more than a question per step.

`verified-by: bravebot_agent::manifest::a_plan_nobody_approved_runs_nothing`
`verified-by: bravebot_agent::manifest::the_plan_prompt_names_where_a_connector_act_sends_its_content`
`verified-by: bravebot_agent::connectors::the_plan_line_shows_the_text_and_title_the_plan_wrote_on_one_line`
`verified-by: bravebot_agent::manifest::approving_a_plan_is_not_approving_its_writes`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_calls_a_vouched_tool_with_the_arguments_it_fixed`
`verified-by: bravebot_agent::permission_mode::a_plan_is_put_to_a_person_in_every_mode_but_bypass`
`verified-by: bravebot_tui::confirm::grants::the_plan_prompt_shows_the_task_and_every_step`
`verified-by: bravebot_tui::confirm::grants::the_plan_prompt_says_what_approving_it_does_and_does_not_do`
`verified-by: bravebot_tui::remote_confirm::a_plan_crosses_with_every_step_and_the_answer_comes_back`
`verified-by: bravebot_core::manifest::a_described_step_names_every_routing_field_it_fixes`
`verified-by: bravebot_core::manifest::a_described_write_names_where_its_content_came_from_unless_that_is_the_workspace`
`verified-by: bravebot_core::manifest::a_step_spells_the_fields_the_contract_names_as_paths_and_no_other`
`verified-by: bravebot_agent::manifest::a_plan_step_draws_a_file_name_spelled`
`verified-by: bravebot_agent::manifest::the_summary_the_question_and_the_record_spell_a_step_alike`
`verified-by: bravebot_core::manifest::a_step_keeps_the_white_space_of_a_name_it_drew`
`verified-by: bravebot_agent::manifest::a_running_step_reports_the_file_it_names_spelled`
`verified-by: bravebot_agent::manifest::a_fetched_slot_is_written_to_a_file_and_answered_as_it_came`
`verified-by: bravebot_agent::manifest::a_connector_act_is_sent_to_the_authority_under_bypass_carrying_no_answer`
`verified-by: bravebot_cli::main::a_plan_is_answered_by_whoever_typed_the_command`
`verified-by: bravebot_cli::main::a_plan_is_refused_where_nobody_can_be_asked`

<a id="MANIFEST-11"></a>
### MANIFEST-11: a run started from a session is a run, not a mode the session holds

A session starts one run, waits for it, and comes back to the turn loop. It is blocked for the
duration: there is no planner in a manifest run to hand a line to, so nothing about the session
carries on beside it, and the person can read, edit and stop but not send.

The conversation is neither read nor written. It does not go in, because the planner that reads the
task may hold the task string and the driver's own words and nothing else (MANIFEST-1); and nothing
comes back out, because a step's result is quarantined and there is no planner left to show it to
(MANIFEST-8). What the transcript shows is the goal as the planner understood it, the frozen plan,
each step as it runs, and the reply, all of it released for a screen and none of it in the exchange
a later turn resumes. So the run leaves the conversation exactly as a declined plan leaves the
workspace.

The run is written down as its own record, by the same code the command line uses, and the session
records its name. That is what keeps the presence of a manifest in a record the thing that makes it
a manifest run: the session's record is still a conversation and still resumes, the run's record
still has no conversation and is still refused by the picker, and neither has to become a question
about which half of a record is being asked about. A run the person stopped is not written, as it is
not from the command line: there is nothing in it to read. That is read off the key they pressed and
not off the error that came back, since the two keys that stop a run reach it as a decline at the
plan prompt and as a cancellation a step later, and a person pressing one of them asked for the same
thing at both moments.

A run returns to its caller by finishing, failing with an error or being stopped. Each of the three
leaves the continuing session with the file decisions the run ended with: attachment grants,
unrelated grants, explicit distrust, and the downgrade of any path a completed write changed. A stop
before any effect keeps the earlier decisions. A downgrade survives a later error or stop, and a
session save and resume. The terminal and the desktop bridge adopt these decisions before the
session continues or is saved. The bridge writes the session's record as the run ends, where the
session has one, so a session closed before its next turn resumes with them, and it reports them in
`trust.rules` on both `manifest.done` and `manifest.error`. Trust comes from the retained authority and effect metadata, never from file
contents or hashes.

A panic or a lost worker is not one of these endings. A release build aborts on a panic, and when
the terminal loses its worker the session keeps the map it held before the run. Recovering the
decisions of a run that ended that way is outside this clause.

Completed planning and processor requests retain their measured usage when a later request or
step fails, including completed replies that cannot be used. Cumulative progress replaces earlier
progress. A successful outcome replaces retained progress, so neither records nor continuing
terminal sessions charge both. Failed run records keep measured wall, inference, tool and approval
time separately. Unknown timing stays absent; the required token total stays zero where nothing
was measured. A stopped run keeps measured usage in the continuing terminal session while leaving
no separate manifest record. Manifest runs do not compact, delegate or vet content. The one check a
run makes is the one over a server's list before the person is asked to vouch for it
([MANIFEST-15](#MANIFEST-15)).

Approval reaches the session prompt rather than the terminal one, since the session is the place a
plan can be drawn and scrolled. Everything else about the question is MANIFEST-10's, unchanged: it
is asked once, it has no standing form, and it does not approve the writes.

A session is also the only place a permission mode exists, so it is the only place a manifest run
meets one. Bypassing approves the plan, as MANIFEST-10 says. Plan mode is the one that refuses: a
plan with a write in it does not run at all, decided from the frozen plan before the plan is put to
anybody ([permission-modes.md](permission-modes.md#MODE-3)).

`verified-by: bravebot_tui::sessions::failed_manifest_retains_file_decisions_live_and_resumed`
`verified-by: bravebot_tui::sessions::cancelled_manifest_retains_file_decisions_live_and_resumed`
`verified-by: bravebot_tui::sessions::successful_manifest_retains_exact_file_decisions`
`verified-by: bravebot_tui::sessions::cancellation_before_manifest_effect_keeps_exact_file_decisions`
`verified-by: bravebot_tui::sessions::a_lost_manifest_worker_leaves_the_session_map_as_it_was`
`verified-by: bravebot_ui_bridge::manifest::ordinary_manifest_endings_keep_file_trust_in_the_bridge`
`verified-by: bravebot_ui_bridge::manifest::a_downgrade_a_run_records_survives_close_and_resume_without_another_turn`
`verified-by: bravebot_session::sessions::failed_manifest_records_distinguish_unknown_timing_from_measured_zero`
`verified-by: bravebot_cli::running::failed_manifest_usage_survives_json_and_reload`
`verified-by: bravebot_cli::running::manifest_usage_controls_survive_storage`
`verified-by: bravebot_tui::sessions::failed_and_stopped_manifest_usage_survives_session_reload`
`verified-by: bravebot_ui_bridge::manifest::a_declined_run_is_read_back_with_the_plan_and_the_reason`
`verified-by: bravebot_ui_bridge::manifest::a_finished_run_is_read_back_with_what_it_did`
`verified-by: bravebot_session::sessions::a_manifest_run_is_recorded_apart_from_the_session`
`verified-by: bravebot_session::sessions::a_session_that_started_a_run_can_still_be_resumed`
`verified-by: bravebot_session::sessions::a_cancelled_run_leaves_no_record`
`verified-by: bravebot_tui::app::a_run_the_person_stopped_is_read_off_the_key_and_not_off_the_error`
`verified-by: bravebot_tui::confirm::grants::escape_at_the_plan_prompt_stops_the_run_and_n_declines_it`
`verified-by: bravebot_tui::app::a_manifest_run_is_not_a_prompt`

<a id="MANIFEST-12"></a>
### MANIFEST-12: planning runs on the run's model and each processing step on the processor model

Both planning calls ([MANIFEST-1](#MANIFEST-1)) run on the model the run was given. A processing
step's call runs on the model [PROC-13](processors.md#PROC-13) names, which is the `processorModel`
setting or the `--processor-model` flag ([CLI-34](cli.md#CLI-34)), and on the run's own model where
neither names one. A plan can then be made on a model chosen for reasoning and carried out on one
chosen for cost, so a run with several transforms over long text is not billed at the planner's price
for each.

The choice is the person's configuration. No plan, step, instruction or file content names a model,
so nothing the run reads decides where a processor's input goes. A processing step is the call it is
in a turn ([PROC-1](processors.md#PROC-1)): no tools, no history and one request. A weaker model can
write a transform worse, and a write of the result is approved from the diff of the real bytes. The
trail records the model each call names ([TRACE-10](trace.md#TRACE-10)).

A processor model that needs a sign-in this machine has not made fails the step that would have used
it. The failure names the model, and nothing is sent to it or to the run's model in its place. A model
the managed layer refuses or that no configured service answers for is refused the same way
([PROC-13](processors.md#PROC-13)).

`verified-by: bravebot_agent::manifest::a_manifest_run_plans_on_the_runs_model_and_processes_on_the_processor_model`
`verified-by: bravebot_agent::manifest::a_manifest_run_with_no_processor_model_processes_on_the_runs_own`
`verified-by: bravebot_agent::manifest::a_manifest_processor_model_needing_a_sign_in_fails_the_step_and_sends_nothing`
`verified-by: bravebot_agent::manifest::a_manifest_processor_answered_by_another_model_than_the_configured_one_says_so`

<a id="MANIFEST-13"></a>
### MANIFEST-13: an authority's detail about a call reaches only the margin

The detail an authority reports about a connector call, for a fetch it did not accept and for an
act whatever its outcome, is drawn in the margin every untrusted byte is drawn in, labelled as the
connector's output. It goes to no model and into no step's failure line, which holds only the
outcome and the authority's reason code, and a reply that does not parse is reported by where it
failed and not by what it held.

An accepted act's link, where the authority gives one, is shown on the step's line only when it is
one `https://` URL of at most 512 printable ASCII characters with no space; any other is dropped,
not shown in part.

**Why.** The detail may quote the service, which is text other people wrote. The link is a
fixed-shape field the authority builds from its definition's template and values it checked, and
its form is held here so that it cannot draw anything but one URL on one line.

`verified-by: bravebot_agent::manifest::a_refused_connector_call_fails_its_step_without_quoting_the_service`
`verified-by: bravebot_agent::manifest::a_reply_that_does_not_parse_fails_its_step_without_quoting_it`
`verified-by: bravebot_agent::manifest::an_authoritys_detail_about_an_accepted_act_is_shown`
`verified-by: bravebot_agent::manifest::an_accepted_acts_link_is_shown_only_when_it_is_well_formed`

<a id="MANIFEST-14"></a>
### MANIFEST-14: a connector act's content is released only to the place its plan fixed

A connector act that carries a slot, or takes a value a fetch recorded, names from the frozen plan
the one connector and place that content may go to and where it came from: a workspace read, a
connector fetch's place, or the union of what a processor read. It is released to that place and
nowhere else, and not to a screen. Content the plan wrote itself releases nothing.

A release fixes the account and the place. Whom an act addresses within that place, such as the
recipient of a reply, is what its definition takes from the item the act lands on: the plan line
names it, and the authority's popup shows it before anything is sent, but the release does not fix
it, and an item a fetch chose without narrowing is one its content chose. Known costs records it.

**Why.** The plan is written from the task before anything is read, so a release fixed from it
lets fetched content change what a slot says and never where it goes.

`verified-by: bravebot_core::manifest::a_connector_act_release_is_named_before_anything_is_read`
`verified-by: bravebot_core::policy::a_connector_release_is_refused_unless_precommitted_to_that_place`
`verified-by: bravebot_agent::manifest::an_act_takes_a_recorded_field_as_the_fetch_s_receipt`
`verified-by: bravebot_core::manifest::a_release_names_what_an_act_carries_and_what_it_takes`

<a id="MANIFEST-15"></a>
### MANIFEST-15: a step may call one tool of a server a person vouched for, with arguments the plan fixed

The step `mcp_call` names a server by the alias a person typed, a tool by its word on a list the
person vouched for ([SERVERS-8](mcp-servers.md#SERVERS-8)), and carries the arguments of the call as
a JSON object the plan wrote. The alias and the tool are routing, locked before the first step as
every routing field is ([MANIFEST-6](#MANIFEST-6)), and are what a person approves a call by
([SERVERS-7](mcp-servers.md#SERVERS-7)). The arguments are the plan's own text and are not routing
([ROUTE-1](routing.md#ROUTE-1)). They are written out in the plan and fixed with it.

The step is an action. It comes after every read and every transform, and it has no field that names
a slot, so no argument of a call can be taken from what an earlier step produced, and a call fills
no slot for a later step to use. No argument can be taken from a choice a processor made either
([ROUTE-10](routing.md#ROUTE-10)): the planner is offered `<argument>_ref` in a turn and not in a
plan, and a step that writes one is refused as naming an argument the tool does not list. An `apply`
step is the one way a plan's calls take values from what a read returned and from a processor's
rows, within the bounds [MANIFEST-16](#MANIFEST-16) sets. What a tool answers is untrusted and
private ([MCP-1](mcp.md#MCP-1)). It is drawn in the margin every untrusted byte is drawn in and goes
to no model and to no step. Pictures and other parts of the answer are not shown and are counted.

The step `mcp_read` makes the same call to a tool the list the person vouched for marks read-only,
which is the server's own `readOnlyHint` annotation, drawn with the tool when the person was asked
about the list. Its alias, tool and arguments are what a call's are, routed, fixed and checked the
same way, and it is decided at its step as a call is. It is a read: it comes before every action
and names the slot its reply fills, quarantined as anything a read fetched is
([MANIFEST-8](#MANIFEST-8)), so a transform can read that slot and an answer can show what the
transform made. It names no slot it takes from, so its arguments are still the plan's literals.
Whatever reply arrives fills the slot, since a reply that arrived ends nothing: a failure the tool
reported is drawn in the margin and its words fill the slot, and a reply this client could not use
fills it with this process's sentence saying so. A read that gets no reply ends the run as a call
that gets none does. A plan naming a tool the list does not mark read-only in an `mcp_read` is refused
before it is put to anybody and asked for once more.

The planner is offered the step only where the session holds a list a person vouched for and a grant
for its server, and is shown the names on that list: each `alias:tool` and each argument by name, its
type and whether it is required, whether the list marks it read-only, and no description. It is
offered `mcp_read` only where some offered tool is marked read-only. Those names are trusted and public
([LABEL-8](labels.md#LABEL-8)), so the planner's context is still the one
[MANIFEST-1](#MANIFEST-1) admits. A list that was declined, a server the run holds no grant for, and
`--tools` offer the planner no call.

A session whose servers nothing has started has them started first, by the function that starts
them for a turn ([SERVERS-4](mcp-servers.md#SERVERS-4), [SERVERS-9](mcp-servers.md#SERVERS-9)). The
terminal and a one-shot run start them as they open. The desktop application starts them at the
session's first turn or first plan run, whichever comes first, and keeps them until the session
closes, so the second of the two starts none. The question about each server, the bound on its
handshake ([SERVERS-15](mcp-servers.md#SERVERS-15)) and a stop at the question are the turn's. A
server the person did not approve is not started. A run stopped at the question starts none and
leaves the session to ask again at its next turn or run. Every server the settings request is
started, whether or not the plan calls one, as a turn's start does. The planner is shown a server's
tools only after its list is vouched for, so the start cannot wait for the plan.

Before the planner is asked for anything, the run puts to the person the list of each server it
holds a grant for that nobody has been asked about in this session, as the start of a turn does
([SERVERS-8](mcp-servers.md#SERVERS-8)). One function decides the check over the list, the answer the
permission mode gives and a stop at the question, so they are the turn's. A yes stands for the rest of
the session and a later run is not asked about the list again. A stop at the question leaves the list
unasked and plans nothing. The question is put under a policy of its own that ends before planning
begins, so what the check read is not in the planner's context. Settling starts no further server and
calls none, and it changes nothing a plan's other steps are shown or hold. A local server that announced a
change to its tools is read again ([MCP-12](mcp.md#MCP-12)) within the bound its declaration gives a
request ([SERVERS-15](mcp-servers.md#SERVERS-15)). A list not given within the bound is not offered,
nobody is asked about it, and a plan naming one of its tools is refused as naming a capability that
was not offered.

Before the plan is put to anybody, every call is held to the list. The tool is on it, the arguments
are a JSON object, every name in the object is an argument the tool lists, and every argument it
requires is there. A plan that fails is asked for once more ([MANIFEST-1](#MANIFEST-1)). The line a
person approves names each call as `alias:tool` and gives its arguments as the server will receive
them, in full.

The call itself is made by the function a turn's call goes through
([SERVERS-7](mcp-servers.md#SERVERS-7)), so everything that decides a turn's call decides this one.
A `deny` rule about the tool refuses it before anybody is asked, in every mode. Of the session's
rules a plan run reads these and no others. Where a person approved the plan and nothing else
decides the call, that approval answers it, and the step puts no question: the person approved this
call with these arguments, and nothing the run reads can change either. A plan bypassing approved
was seen by nobody, so its calls are decided by the mode as a turn's are. What would put the
question in spite of a standing answer still puts it at the step: an `ask` rule about the tool, a
call carrying private data, and plan mode ([MODE-3](permission-modes.md#MODE-3)). So does a call
whose arguments hold a character the step's question writes out
([PROMPT-13](prompting.md#PROMPT-13)), since the line the person approved could not show it. The egress gate decides as it does there.

A server declared to ask every call ([SERVERS-21](mcp-servers.md#SERVERS-21)) is asked at the step,
in every mode, bypassing included, and no answer to it lasts. The plan is not refused for holding
such a call. Refusing it at plan time would not make any call safer, since each is asked at its step
either way, and it would decide [SERVERS-21](mcp-servers.md#SERVERS-21) in a second place that has to
be kept equal to the first. Where nobody can be asked the call is refused at the step. Plan mode does
not refuse the plan for a call. It asks at the step as it does in a turn, which is what
[MODE-3](permission-modes.md#MODE-3) does not change.

A call ends the run when it is refused (by a rule, the permission mode, the egress gate or a
context that has met untrusted content), declined or not asked about by the person, not offered to
the run, not made (no server was reached, or it could not be started or written to), or not
answered (its connection closed before any reply, or no reply came within the bound the server's
declaration gives, [SERVERS-15](mcp-servers.md#SERVERS-15)). No step after it runs, and the steps
before it are not undone. The step is recorded as failed and no later step is recorded.

A reply that arrived ends nothing, whatever it holds. That covers a result, a tool that answers
that it failed, a JSON-RPC error, a reply this client cannot read, an HTTP status that is not a
success, and a line past the bound ([MCP-5](mcp.md#MCP-5), [MCP-13](mcp.md#MCP-13)). The steps
after it run, because the steps were fixed before the server was reached and what a server returns
does not change them. Within an `apply` step, a call the server reported failed skips the calls of
that step that depend on it ([MANIFEST-16](#MANIFEST-16)). A tool's account of its own failure is drawn in the margin
([MANIFEST-13](#MANIFEST-13)). A reply this client could not use is drawn as nothing, since the
client holds none of it, and the step's line says that the call failed and its reply could not be
used. No word of the reply is in the line or in the record of the run.

A server declared to run in an OpenShell sandbox ([SERVERS-24](mcp-servers.md#SERVERS-24)) is called by
a plan run and by nothing else: its list is put before the plan as any server's is, and a step calls
it as above. A turn neither puts that list nor offers its tools.

**Why.** A call to a server is an effect decided by a program the plan does not name, so it is an
action and it is approved by what a person can decide alone, the alias and the tool, with the
arguments shown beside them. A plan has read nothing, so every argument is the planner's literal,
and giving the step no slot is what makes that structural and not a promise. Sending the call
through the one function that decides a turn's call keeps a `deny` rule and a declaration asking
every call in force in a plan without a copy of either here. Asking again at the step would put the
question the plan's line already put, about the same call with the same arguments, and a question
that can only repeat an answer teaches a person to give it without reading. A read is a fetch from
a program too, but one its server says changes nothing, so it can stand where reads stand. Its reply
is held as a fetched file is, so the planner still reads none of it, and its arguments are still
fixed before anything is read. The server's word that a tool only reads is trusted no further than
the order of the plan: the call is approved as any planned call is, and a server that marks an
acting tool read-only gains an earlier place in the plan for a call the person approved, never a
call nobody approved.

`verified-by: bravebot_agent::manifest_server_calls::a_plan_calls_a_vouched_tool_with_the_arguments_it_fixed`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_started_in_a_fresh_session_puts_the_lists_to_the_person_before_it_plans`
`verified-by: bravebot_agent::manifest_server_calls::bypassing_answers_a_list_at_the_start_of_a_plan_and_makes_no_check`
`verified-by: bravebot_agent::manifest_server_calls::a_list_the_person_declines_at_the_start_of_a_plan_offers_no_call`
`verified-by: bravebot_agent::manifest_server_calls::only_the_lists_of_the_servers_a_run_holds_are_put_to_the_person`
`verified-by: bravebot_agent::manifest_server_calls::settling_a_list_leaves_a_plan_that_calls_nothing_as_it_was`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_stopped_at_a_lists_question_plans_nothing_and_leaves_the_list_unasked`
`verified-by: bravebot_agent::manifest_server_calls::a_server_that_does_not_give_its_changed_list_in_time_does_not_hold_up_a_plan`
`verified-by: bravebot_agent::manifest_server_calls::the_planner_is_shown_a_vouched_lists_names_and_none_of_its_words`
`verified-by: bravebot_agent::manifest_server_calls::a_call_the_schema_or_the_list_cannot_hold_runs_nothing`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_the_list_refuses_is_asked_for_once_more`
`verified-by: bravebot_agent::manifest_server_calls::an_ask_rule_puts_a_planned_call_to_the_person_at_its_step`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_bypass_approved_answers_no_call_for_a_person`
`verified-by: bravebot_agent::manifest_server_calls::a_planned_call_holding_a_hidden_character_is_asked_at_its_step`
`verified-by: bravebot_core::policy::approving_a_plan_answers_the_default_question_of_a_call_it_fixed_and_nothing_else`
`verified-by: bravebot_agent::manifest_server_calls::a_planned_call_to_a_server_asked_every_call_asks_at_its_step`
`verified-by: bravebot_agent::manifest_server_calls::bypassing_does_not_answer_a_planned_call_to_a_server_asked_every_call`
`verified-by: bravebot_agent::manifest_server_calls::plan_mode_runs_a_plan_that_calls_a_server_and_asks_at_the_step`
`verified-by: bravebot_agent::manifest_server_calls::a_refused_call_does_not_run_the_next_step`
`verified-by: bravebot_agent::manifest_server_calls::a_tool_that_reports_a_failure_is_drawn_in_the_margin_and_decides_nothing`
`verified-by: bravebot_agent::manifest_server_calls::a_reply_this_client_cannot_use_does_not_end_a_plan_run`
`verified-by: bravebot_agent::manifest_server_calls::a_line_past_the_bound_and_the_calls_after_it_do_not_end_a_plan_run`
`verified-by: bravebot_agent::manifest_server_calls::a_call_that_gets_no_reply_ends_a_plan_run`
`verified-by: bravebot_agent::manifest_server_calls::a_deny_rule_refuses_a_planned_call_in_every_mode_and_asks_nobody`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_cannot_name_a_choice`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_reads_a_tool_marked_read_only_into_a_slot_and_answers_from_it`
`verified-by: bravebot_agent::manifest::a_read_is_offered_only_where_a_tool_is_marked_read_only`
`verified-by: bravebot_agent::manifest_server_calls::a_read_whose_reply_arrived_fills_its_slot_whatever_it_holds`
`verified-by: bravebot_agent::manifest_server_calls::a_declined_or_denied_read_ends_the_run_before_the_steps_after_it`
`verified-by: bravebot_core::manifest::a_server_read_fills_a_slot_a_later_step_uses`
`verified-by: bravebot_core::manifest::a_server_read_comes_before_every_action_and_takes_from_no_slot`
`verified-by: bravebot_agent::manifest::a_calls_arguments_object_is_carried_as_its_text`
`verified-by: bravebot_core::manifest::a_server_call_locks_its_alias_and_tool_and_not_its_arguments`
`verified-by: bravebot_core::manifest::a_server_call_names_no_slot_so_no_argument_comes_from_a_result`
`verified-by: bravebot_core::manifest::a_server_call_with_an_extra_field_or_a_missing_name_is_refused`
`verified-by: bravebot_core::manifest::a_server_calls_arguments_must_be_text`
`verified-by: bravebot_core::manifest::a_read_after_a_server_call_is_refused`
`verified-by: bravebot_core::policy::a_plan_calling_a_server_from_a_tainted_context_is_refused`
`verified-by: bravebot_core::permissions::only_mcp_keeps_the_rules_about_a_servers_tool_and_nothing_else`
`verified-by: bravebot_ui_bridge::manifest::a_plan_run_before_any_turn_starts_the_sessions_servers_and_calls_a_tool`
`verified-by: bravebot_ui_bridge::manifest::a_plan_run_does_not_start_a_server_the_window_refuses`
`verified-by: bravebot_ui_bridge::manifest::a_turn_after_a_plan_run_finds_the_servers_the_run_started`
`verified-by: bravebot_ui_bridge::manifest::a_plan_run_after_a_turn_finds_the_servers_the_turn_started`
`verified-by: bravebot_ui_bridge::manifest::a_plan_run_stopped_at_a_servers_question_starts_nothing_and_the_next_run_asks_again`
`verified-by: bravebot_ui_bridge::manifest::a_server_that_never_answers_its_handshake_does_not_hold_up_a_plan_run`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_puts_and_calls_the_list_of_a_server_only_a_manifest_run_calls`

<a id="MANIFEST-16"></a>
### MANIFEST-16: an `apply` step makes the calls a processor proposes, of tools and lists the plan fixed

The step `apply` names a server, its tools, the lists a proposal may point into and, for each
argument of each tool, the one way a value may reach it: the plan's own literal, a field of an entry
of a list copied from what a read returned, that field of several entries as an array, short new
text the processor writes, or the new text an earlier row of the step gave. The server and the tools
are routing, locked as every routing field is ([MANIFEST-6](#MANIFEST-6)). Every list is the slot of
an earlier read. A kind other than literal is allowed only where the person's declaration of the
server names that tool and argument with it ([mcp-servers.md](mcp-servers.md#SERVERS-26)), and a
plan that gives one the declaration does not name is refused before it is put to anybody. So is a
step that gives no argument such a kind, since its calls would take nothing from the list.

An isolated processor reads the lists, each numbered from 1, and answers with rows. Only the entries
holding a field an argument takes from that list are numbered, the first 999 of them. A row names a
tool by its number, each entry by its list and number, new text by its number, an earlier row by its
number. A row holds no address, alias or path. The answer is read by the reader that reads a
processor's number ([processors.md](processors.md)), outside the driver. A row it cannot read, or
that gives an argument in a way the plan does not allow, names what is not there, repeats a call or
passes a cap (`max_rows`, 50 by default and at most 100; `max_new`, 12 by default and at most 50),
is dropped and counted by reason. So is a row giving a value that is not the type the server's list
declares for its argument, and every value is sent as that type. An answer with no row, `none`
included, or whose rows were all dropped, makes no call and ends the step without failing the run.
An answer with no content is asked for once more, and a second ends the step the same way: unlike a
process step's ([MANIFEST-8](#MANIFEST-8)), it fills no slot a later step needs. New text is one
line of at most 64 characters with every control and every character that draws as nothing made a
space. A call depends on at most one earlier call, the one whose new text it uses.

The resolved calls are put to the person as one question, each value drawn with where it came from
and as untrusted content is drawn, and no rule, standing answer or mode answers it, bypassing
included. Only the terminal interface puts the list; in any other client, and where nobody can be
asked, the step is refused. The calls the person keeps are made in row order, each through a grant
bound to its server, tool and place in the list, and a `deny` rule is read before each. A call whose
tool reports a failure skips the calls that depend on it and ends nothing. A call refused or not
made ends the run. What became of every call is drawn after them, and reaches no model.

**Why.** The planner cannot know which items a task touches without reading them, and must never
read them. Numbers leave nothing to imitate, so every target a call acts on is copied from a list a
read returned, and the person's answer to the resolved list is what selects the calls. That answer
is also the question [PERM-9](permissions.md#PERM-9) asks before private content reaches a program,
since the list shows every value each call sends, and a server that may hold `--apply` is a local
stdio server, so no egress gate stands between the step and it.

`verified-by: bravebot_agent::manifest_server_calls::an_apply_step_makes_the_calls_the_person_approved_as_one_list`
`verified-by: bravebot_agent::manifest_server_calls::unticking_a_call_unticks_what_depends_on_it_and_declining_makes_nothing`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_the_declaration_does_not_allow_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_of_literals_alone_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_naming_what_the_vouched_list_does_not_hold_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_whose_arguments_are_malformed_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_whose_entries_are_malformed_makes_no_call`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_whose_list_nobody_can_answer_makes_no_call`
`verified-by: bravebot_agent::manifest_server_calls::a_deny_rule_refuses_an_apply_steps_call_the_list_approved`
`verified-by: bravebot_core::apply::applying_keeps_what_its_dependencies_allow_and_a_failure_skips_its_dependants`
`verified-by: bravebot_core::policy::an_applied_call_is_carried_once_and_only_where_the_person_kept_it`
`verified-by: bravebot_choice::lib::a_processor_that_was_not_asked_for_rows_has_none_read_from_its_answer`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_into_a_list_a_model_wrote_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_whose_call_could_depend_on_two_rows_is_refused`
`verified-by: bravebot_agent::manifest_server_calls::a_move_may_land_in_a_folder_the_step_made_or_one_the_list_holds`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_step_asks_its_processor_again_after_an_empty_reply`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_step_whose_processor_proposes_nothing_makes_no_call_and_fails_nothing`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_step_whose_processor_finds_nothing_to_do_says_so`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_step_triages_an_issue_tracker_with_a_tool_chosen_per_row`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_step_labels_issues_with_labels_from_a_second_list`
`verified-by: bravebot_agent::manifest_server_calls::an_apply_step_fills_an_array_argument_from_several_entries`
`verified-by: bravebot_choice::rows::a_list_numbers_only_the_entries_that_hold_a_needed_field`
`verified-by: bravebot_choice::rows::rows_resolve_to_calls_with_values_from_the_list_and_the_processor`
`verified-by: bravebot_choice::rows::a_row_the_plan_does_not_allow_is_dropped_and_counted`
`verified-by: bravebot_choice::rows::an_argument_of_several_kinds_takes_the_one_the_row_gave`
`verified-by: bravebot_choice::rows::a_wrapped_list_is_found_at_its_pointer_and_numbers_count`
`verified-by: bravebot_choice::rows::new_text_is_cleaned_and_the_caps_hold`
`verified-by: bravebot_choice::rows::a_value_is_sent_as_its_declared_type_and_a_row_that_is_not_is_dropped`
`verified-by: bravebot_choice::rows::rows_point_into_several_lists_and_take_arrays_of_entries`

<a id="MANIFEST-17"></a>
### MANIFEST-17: a step whose tools are all marked unattended runs on a person's approval of the plan

Where every tool of an `apply` step is marked unattended in the person's declaration of its server
([mcp-servers.md](mcp-servers.md#SERVERS-26)), and a person answered the plan's question, the step's
resolved calls are made with no question at the step, within the caps the plan line stated, and what
became of each is drawn as for any `apply` step. The plan's line for such a step says it runs
without asking, and names the server each of its lists was read from and the server its calls go to,
so approving the plan releases the lists' private entries to that server as it releases a connector
act's content ([MANIFEST-14](#MANIFEST-14)).

The list is still put, as for any `apply` step, where bypass mode answered the plan, in plan mode,
and where an `ask` rule names one of the step's tools. Who answered the plan is taken when it is
answered, so a later change of mode does not change it.

**Why.** A tool a person marks is one whose every call, within the caps, is acceptable on its own:
it acts only on their own machine, deletes or overwrites nothing, and adds no address. Every target
is copied from a list a read returned and no call falls outside the tools the plan names, so the
worst a hostile entry can choose is which of those acceptable calls are made. That bound holds
whether or not the step asks, which is what lets a person approve a run once and not again at each
step.

`verified-by: bravebot_agent::manifest_server_calls::a_step_whose_tools_are_all_marked_runs_on_the_plans_approval`
`verified-by: bravebot_agent::manifest_server_calls::a_marked_step_puts_its_list_where_a_tool_is_unmarked_a_mode_answered_or_a_rule_asks`

## Known costs

- **A plan answered at a terminal is answered on one line, with no way back to the steps above it.**
  The terminal question prints the plan and reads a line, so a plan longer than the window is
  scrolled to in the terminal's own scrollback rather than in anything this tool draws, and there is
  no going back to re-read a step once the answer is typed. A run started from a session does not
  have this cost: that prompt is drawn and scrolled (MANIFEST-11).
- **A processing step's instruction is not on the line.** What the person reads is which slots it
  reads and which it fills, and the sentence a processor is given is left off, so a plan that
  transforms text under an instruction nobody read still passes the gate. It routes nothing, which
  is why it is a cost rather than a hole in MANIFEST-10: a processor's output is a slot, and every
  step that lands a slot anywhere names the destination on its own line and is put to the person
  again as its turn comes.
- **A processor model that needs a sign-in, or that only the setting names and nothing can use,
  fails the run at its first processing step.** The two planning calls come first and have been
  spent on the run's model by then. A model named with `--processor-model` is checked before the run
  for a refusal by the machine and for no service answering for it ([CLI-34](cli.md#CLI-34)). The
  setting, and a sign-in for either, are met when a processor is first asked
  ([PROC-13](processors.md#PROC-13)): a run that starts no processor has no use for the model and is
  not refused for it, and the run's own sign-in comes first and may cover the model.
- **A release fixes the account and the place, not whom an act addresses there.** An act whose
  definition takes a value from the item it lands on, such as a reply's recipient, addresses whoever
  that item names. Where the fetch that chose the item did not narrow, as a fetch of the newest
  message does not, the content fetched decides who that is. The plan line names what the authority
  fills in and from which fetch, and the authority's popup shows the recipient before anything is
  sent, which is the person's check; nothing earlier refuses it. Requiring such an act's fetch to
  narrow, as an act on every match must, would close it, at the cost of definitions that list
  narrowing terms for every act that takes a value from its item (MANIFEST-14).
- **A call's answer cannot be used by a later step unless its tool is marked read-only.** A call
  fills no slot, so nothing a tool that may act returned can be transformed, written or answered
  with. The person reads it in the margin as the run goes. A tool its server marks read-only can be
  read into a slot instead, and no argument of a call can depend on what any tool returned
  (MANIFEST-15). An `apply` step's can, but only as an entry copied from a list a read returned or
  short new text its processor wrote, where the server's declaration allows it (MANIFEST-16). A
  plan that needs any other use of what a tool returned is two runs, with the person's own words
  between them.
- **A plan run starts servers and asks about lists whether or not the plan calls a server.** The
  questions come before the plan is made, so a person who runs a plan that calls nothing is asked
  once about each server that nothing has started, in the desktop application, and about each list
  in the session that nobody has been asked about. A run with nobody to ask answers no, and its
  servers offer no call unless permissions are skipped (MANIFEST-15).
- **A reply that arrived does not stop the plan.** The steps after a call whose reply was a tool's
  report of failure, a JSON-RPC error, a reply this client could not read, an HTTP status that is
  not a success or a line past the bound run, since stopping on any of them would let the server's
  reply decide which steps happen. A plan whose later steps depend on an earlier call having worked
  runs them anyway. The person sees a tool's account of its failure in the margin as it goes, and
  for a reply this client could not use only a line saying so (MANIFEST-15). An `apply` step's
  calls that depend on a failed call are the exception: they are skipped (MANIFEST-16).
- **Only the terminal interface puts an `apply` step's list.** The desktop application, line mode
  and a one-shot run cannot draw it, so an `apply` step there is refused (MANIFEST-16).
- **A server that sends no reply can end a plan run.** A call whose connection closes before any
  reply, or that is not answered within its bound, ends the run at that step, so a server decides
  by not answering whether the later steps run. No byte it wrote takes part, a server that is not
  there ends the run the same way, and what it can do is keep approved steps from running, never
  add one (MANIFEST-15).
- **A plan holding a call to a server asked every call stops at that step where nobody can be
  asked.** A one-shot run and the desktop application refuse the call, and the steps before it have
  run (MANIFEST-15, [SERVERS-21](mcp-servers.md#SERVERS-21)).
- **Approving a plan approves its calls to servers.** A person who approves a plan without reading
  its lines has approved every call on them. Each line gives the call's alias, tool and arguments in
  full, and a call an `ask` rule names, one carrying private data, and one to a server declared to
  ask every call are still put at their step.

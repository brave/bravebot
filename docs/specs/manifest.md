---
id: MANIFEST
title: Plan then execute
status: normative
governs:
  - crates/core/src/manifest.rs
  - crates/core/src/policy.rs
  - crates/agent/src/manifest.rs
  - crates/agent/src/mode.rs
  - crates/cli/src/main.rs
  - crates/session/src/sessions.rs
  - crates/tui/src/resume.rs
  - crates/tui/src/confirm.rs
  - crates/tui/src/app.rs
guards:
  - symbol: Policy::before_planning
  - symbol: Policy::adopt_manifest
  - symbol: Policy::quarantine
  - symbol: Manifest::routing
documented-by:
  - docs/website/docs/using/headless.md
  - docs/website/docs/using/sessions.md
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
person pasted into it or dropped onto it, and the driver's own words, and has been shown nothing
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
a step has read something.

`verified-by: bravebot_core::policy::every_planning_call_is_allowed_from_a_clean_context`
`verified-by: bravebot_core::policy::a_planner_shown_untrusted_content_may_not_plan`
`verified-by: bravebot_agent::manifest::the_audit_trail_records_each_planning_call`
`verified-by: bravebot_agent::manifest::a_manifest_that_is_not_json_fails_without_another_call`
`verified-by: bravebot_agent::manifest::a_picture_pasted_into_the_task_reaches_the_planner`
`verified-by: bravebot_agent::manifest::a_picture_dropped_onto_the_task_reaches_the_planner`

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

<a id="MANIFEST-4"></a>
### MANIFEST-4: validation is pure and total

The validator sees a draft and nothing else. Any violation fails the run whole. A manifest is
never half adopted, and a step is never repaired to make a plan usable.

`verified-by: bravebot_core::manifest::an_empty_plan_is_refused`
`verified-by: bravebot_core::manifest::a_tool_outside_the_schema_is_refused`
`verified-by: bravebot_core::manifest::a_path_outside_the_workspace_is_refused`
`verified-by: bravebot_core::manifest::a_read_after_an_action_is_refused`
`verified-by: bravebot_core::manifest::validation_is_deterministic`
`verified-by: bravebot_agent::manifest::a_plan_that_fails_validation_runs_nothing`

<a id="MANIFEST-5"></a>
### MANIFEST-5: the driver adds nothing

It may not insert, skip, reorder, or synthesise a step, and there is no handler for a tool the
schema does not name. If a plan cannot express something, the plan is wrong, not the driver.

`edit_file` is unavailable, because locating a passage means having read the file and the planner
has read nothing. `todo_write` is unavailable because the manifest is already the task list.
There is no shell and no `run`: a command string is destination and payload at once.

`verified-by: bravebot_agent::manifest::injected_text_in_a_file_cannot_add_a_step`
`verified-by: bravebot_agent::manifest::the_planner_is_shown_capabilities_and_no_tool_names`
`verified-by: bravebot_agent::manifest::the_registry_and_the_kernel_schema_describe_the_same_tools`
`verified-by: bravebot_core::manifest::no_action_in_the_schema_fills_a_slot`

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
nothing.

`verified-by: bravebot_core::manifest::every_effect_destination_is_named_for_the_routing_lock`
`verified-by: bravebot_core::manifest::an_omitted_search_directory_is_locked_as_the_workspace`
`verified-by: bravebot_core::manifest::an_omitted_list_pattern_is_locked_as_no_filter`
`verified-by: bravebot_core::manifest::a_pattern_the_plan_left_empty_is_locked_as_no_filter`
`verified-by: bravebot_core::manifest::a_routing_field_that_is_not_text_is_refused`
`verified-by: bravebot_core::manifest::a_routing_default_never_replaces_a_field_the_plan_gave`
`verified-by: bravebot_agent::manifest::a_plan_whose_routing_field_is_not_text_is_refused`
`verified-by: bravebot_agent::manifest::a_write_lands_where_the_plan_said_and_carries_what_it_never_read`
`verified-by: bravebot_agent::manifest::a_listing_with_no_pattern_lists_the_tree`

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

An unmarked processor answer is a remark, not a document. Treating it as the slot would write an
explanation over whatever the plan named next.

`verified-by: bravebot_core::policy::a_step_result_is_quarantined_whatever_its_label`
`verified-by: bravebot_agent::manifest::an_unmarked_transform_does_not_become_a_file`
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
directory a search runs in and the glob it filters by, not the pattern alone. Declining stops the run,
and what was declined comes back with the error under MANIFEST-3, so the plan can still be read.
Nothing has been read or written by then, so a declined run leaves the workspace exactly as it was.

The question is asked once, and there is no standing form of it. Nothing between the plan and the
run can reshape the plan, so there is nothing to ask a second time; and a plan is written afresh for
each run, so remembering an answer would be approving steps nobody has seen.

Approving a plan is not approving its writes. This mode widens the scope of the precommitment and
does not replace the gates inside it, so each write is still put to the person as its step reaches it.

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
`verified-by: bravebot_agent::manifest::approving_a_plan_is_not_approving_its_writes`
`verified-by: bravebot_agent::permission_mode::a_plan_is_put_to_a_person_in_every_mode_but_bypass`
`verified-by: bravebot_tui::confirm::the_plan_prompt_shows_the_task_and_every_step`
`verified-by: bravebot_tui::confirm::the_plan_prompt_says_what_approving_it_does_and_does_not_do`
`verified-by: bravebot_tui::remote_confirm::a_plan_crosses_with_every_step_and_the_answer_comes_back`
`verified-by: bravebot_core::manifest::a_described_step_names_every_routing_field_it_fixes`
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

Approval reaches the session prompt rather than the terminal one, since the session is the place a
plan can be drawn and scrolled. Everything else about the question is MANIFEST-10's, unchanged: it
is asked once, it has no standing form, and it does not approve the writes.

A session is also the only place a permission mode exists, so it is the only place a manifest run
meets one. Bypassing approves the plan, as MANIFEST-10 says. Plan mode is the one that refuses: a
plan with a write in it does not run at all, decided from the frozen plan before the plan is put to
anybody ([permission-modes.md](permission-modes.md#MODE-3)).

`verified-by: bravebot_session::sessions::a_manifest_run_is_recorded_apart_from_the_session`
`verified-by: bravebot_session::sessions::a_session_that_started_a_run_can_still_be_resumed`
`verified-by: bravebot_session::sessions::a_cancelled_run_leaves_no_record`
`verified-by: bravebot_tui::app::a_run_the_person_stopped_is_read_off_the_key_and_not_off_the_error`
`verified-by: bravebot_tui::app::a_manifest_run_is_not_a_prompt`

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

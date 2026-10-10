---
id: TRACE
title: The trace
status: normative
governs:
  - crates/core/src/event.rs
  - crates/session/src/audit.rs
  - crates/ui-bridge/src/turn.rs
  - crates/agent/src/request_view.rs
documented-by: docs/website/docs/security/audit-trail.md
---

## Scope

What is recorded about every decision the system makes, what that record may contain, and how it
is read back.

A **gate** is a check that has to pass before anything consequential happens: content reaching the
model, a file being written, a program being run, a request leaving the process. Each one decides a
single question and refuses rather than warning, so there is no path to a consequence that does not
go through one. The trail is the record of those decisions.

## Clauses

<a id="TRACE-1"></a>
### TRACE-1: every gate decision is recorded, allowed or refused

A refusal is as much a record as a permission. A read and a write leave different trails, a
promotion is recorded as one, and the fields fixed before a turn observed anything are recorded
first. A promotion records the label it decided and nothing about where the path lands. Where a
tool tells the planner that a path resolves outside the workspace, the trail records that refusal
as well, with the remedy offered and the path named as the planner was told it: `ref:N` for a
reference, the path otherwise.

A delegate the gate approved and its checkout then refused is recorded as a refusal under
`delegate`, not as a permission. The entry gives the delegate's number, one fixed cause (the
isolation argument was not `checkout`, a reader asked for one, the run already works in one, no
state directory, the checkouts would sit inside the working directory, an opened directory holds
the working directory, an opened directory holds the checkouts, the directory for checkouts is
unusable, or the repository was refused), the directory a person opened where that was the cause,
and how many of the call's delegates were not started, the withdrawn one included. The delegate
still takes no number and no place under the ceiling ([DELEGATE-7](delegation.md#DELEGATE-7)).

**Why.** A trail that logged only what happened would not answer "why did it not do the thing I
asked", which is most of what anyone asks it.

`verified-by: bravebot_core::policy::a_read_and_a_write_leave_different_trails`
`verified-by: bravebot_core::policy::promotion_appears_in_the_audit_trail`
`verified-by: bravebot_core::policy::the_audit_trail_records_the_precommit_first`
`verified-by: bravebot_core::policy::a_turn_cannot_begin_without_routing`
`verified-by: bravebot_core::policy::a_withdrawn_delegate_is_a_refusal_with_its_cause`
`verified-by: bravebot_core::policy::a_withdrawn_delegate_takes_neither_a_place_nor_a_number`
`verified-by: bravebot_agent::turn::a_spawn_asking_for_a_checkout_it_may_not_have_starts_nothing`
`verified-by: bravebot_agent::workspace::a_checkout_refusal_carries_which_opened_directory_held_what`
`verified-by: bravebot_agent::turn::a_read_refused_for_leaving_the_workspace_is_recorded_as_a_refusal`
`verified-by: bravebot_agent::turn::a_read_through_a_reference_refused_for_leaving_the_workspace_is_recorded_as_the_reference`
`verified-by: bravebot_agent::turn::a_picture_refused_for_leaving_the_workspace_is_recorded_as_a_refusal`
`verified-by: bravebot_agent::turn::every_file_tool_records_a_path_refused_for_leaving_the_workspace`
`verified-by: bravebot_agent::turn::an_edit_whose_file_leaves_the_workspace_while_asked_is_recorded_as_a_refusal`
`verified-by: bravebot_agent::tools::a_deferred_read_refused_for_leaving_the_workspace_is_recorded_as_the_reference`
`verified-by: bravebot_agent::tools::a_directory_outside_the_workspace_is_recorded_as_a_refusal`
`verified-by: bravebot_agent::tools::a_redirection_outside_the_workspace_is_recorded_as_a_refusal`

<a id="TRACE-2"></a>
### TRACE-2: the trail holds no content

Every field is a gate name, a capability, a label, a path, a destination host or a slot id.
Network decisions omit URL userinfo, paths, queries and fragments. That is why it can be shown
on a screen and written to a file without any release, and it is what makes the record safe to keep
for a workspace nobody vouched for.

`verified-by: bravebot_agent::turn::nothing_recorded_about_a_request_carries_the_credential_in_its_url`
`verified-by: bravebot_agent::turn::the_trail_records_the_slot_and_the_path_rather_than_the_content`
`verified-by: bravebot_agent::turn::a_fetch_records_the_host_and_none_of_the_rest_of_the_url_in_the_trail`

<a id="TRACE-3"></a>
### TRACE-3: an assertion a person made is recorded as one

Vouching for the output of a command the user typed, labelling their configuration, and admitting a
pasted picture are each written down, because each is a claim a human made rather than something
the system worked out.

**Why.** These are the points where trust enters from outside. A trail that recorded only what the
system deduced would omit exactly the decisions somebody might later want to account for.

`verified-by: bravebot_core::policy::trusting_a_typed_commands_output_is_recorded_in_the_audit_trail`
`verified-by: bravebot_core::policy::labelling_configuration_is_recorded_in_the_audit_trail`
`verified-by: bravebot_core::policy::a_pasted_image_is_recorded_in_the_audit_trail`

<a id="TRACE-4"></a>
### TRACE-4: on disk it is one JSON object per line, with the axes written out in words

A session's trail is appended a turn at a time, so a line-oriented file can be read with whatever
is to hand. The labels are spelled out rather than abbreviated, because a file read months later
has no legend beside it. Each event keeps the time it happened, and an event a delegate's gate
took keeps the number of that run ([DELEGATE-13](delegation.md#DELEGATE-13)). The turn's own
records carry no such field.

**Why.** The compact form suits a terminal, where the reader has the legend in front of them. A
file has a different reader.

`verified-by: bravebot_tui::sessions::the_audit_keeps_the_time_each_event_happened`
`verified-by: bravebot_session::audit::the_written_record_names_the_delegate_that_took_the_decision`
`verified-by: bravebot_ui_bridge::audit::the_trail_the_front_end_keeps_names_the_delegate_that_took_the_decision`

A **gate** is a check that has to pass before anything consequential happens: content reaching the
model, a file being written, a program being run, a request leaving the process. Each one decides a
single question and refuses rather than warning, so there is no path to a consequence that does not
go through one. Every decision a gate makes is recorded, and the blocks below are those records.

Three pieces of notation appear in them. `(T,pub)` and `(U,priv)` are the label on a value: trusted
or untrusted on the first axis, public or private on the second. `ref:N` is a slot holding content
the planner is not allowed to read, so it is handed the reference instead of the bytes. And
`routing` marks the part of a call that decides where it lands, as opposed to the part that is
merely carried.

Reading a file in a trusted directory, where the content reaches the model:

```
ok      precommit: routing fields ["task"] fixed before any observation
ok      promote: read_file.path proposed by the model, public and non-destructive
ok      file_read.path [routing] (T,pub)
observe file_read produced (T,priv)
ok      trust: notes.md read as trusted, from a trusted path
ok      render: read_file: content reshaped without being read, still (T,priv)
ok      present: tool_result: notes.md is (T,priv), so the planner may read it
```

The same read where nothing is vouched for, so the content is quarantined instead:

```
observe file_read produced (U,priv)
ok      trust: notes.md read as untrusted
slot    ref:0 at (U,priv)
ok      present: tool_result: notes.md is (U,priv), quarantined as ref:0; the planner
        sees a reference only
```

Changing that same file, which nothing along the way is able to read:

```
ok      reference: spawn_processor.reads names ref:1
ok      processor: processor over ref:1 reads ref:1 and writes (U,priv), with no tools,
        no memory and nothing to write but that one slot
ok      processor: input assembled from 1 slot(s) inside the kernel
ok      processor: output labelled (U,priv) by taint over its inputs
ok      render: processor: content reshaped without being read, still (U,priv)
slot    ref:3 at (U,priv)
ok      present: tool_result: quarantined as ref:3; the planner sees a reference only
ok      resolve: write_file: ref:3 resolved to its quarantined content, (U,priv)
release ref:3 (U,priv) -> (U,pub)
ok      declassify: ref:3 released into src/config.py, which is inside the workspace
ok      approval: src/config.py: a path nobody has vouched for either way, asking
```

A read of a file outside the workspace, refused when the path resolves:

```
ok      promote: read_file.path proposed by the model, public and non-destructive
BLOCK   confine: read_file.path: '/etc/hosts' resolves outside the workspace; remedy offered:
        open its directory, or drop the file
```

<a id="TRACE-5"></a>
### TRACE-5: the trail is readable live and after the fact

`--trace` on a one-shot run prints the same thing, and Ctrl-T toggles it in a session. Each line is
one gate that ran: what it checked, the label it saw, and what it allowed. It is the fastest way to
find out why something was refused.

`verified-by: bravebot_tui::app::ctrl_t_toggles_the_trail`
`verified-by: bravebot_tui::render::the_trail_is_hidden_by_default`
`verified-by: bravebot_tui::render::a_blocked_gate_is_shown_in_the_trail`
`verified-by: bravebot_cli::main::the_trail_renders_a_line_for_every_event`
`verified-by: bravebot_cli::running::a_one_shot_run_that_failed_prints_its_trail_under_trace`

<a id="TRACE-6"></a>
### TRACE-6: each planning call is recorded, like any other gate

A manifest run makes two of them, and both appear in the trail: one for the goal in plain words,
and one for fitting that to the tool set. A refusal is as much a record as a permission.

`verified-by: bravebot_agent::manifest::the_audit_trail_records_each_planning_call`

<a id="TRACE-7"></a>
### TRACE-7: the trail names the mode a turn began in and what answered each prompt

Each turn records the permission mode it began with ([permission-modes.md](permission-modes.md)) by
its name, before any tool is called. When the policy decides a command or a write is to be put to
the person, a second entry under `approval` says what answered: the mode that answered in their
place and so drew nothing, or no mode, which leaves it to whatever confirms (a person, or nothing that
can ask). The entries before it that end in "asking" say what the policy decided, and they read as
a person having been asked.

Other prompts a mode answers, such as a fetch or a server start, are not named here.

Answers that were never a prompt already say so in their own entry: a rule in the settings file, a
command the user vouched for, and the audited table.

**Why.** A session started with the flag that skips permissions and one that never skipped them look
the same once the key has moved off bypass, and the screen keeps nothing. The trail outlives it, and
it is the only place somebody accounting for what ran can read which of the two they are looking at.
The names are the modes' own text, so the entry holds no content (TRACE-2).

`verified-by: bravebot_agent::turn::a_turn_records_the_mode_it_began_with`
`verified-by: bravebot_agent::turn::an_approval_names_what_answered_it`

<a id="TRACE-8"></a>
### TRACE-8: the trail says how each delegate ended and why

When the turn collects a delegate, it records the end under `delegate` as its own entry, with the
delegate's number first, like the entry that started it ([DELEGATE-13](delegation.md#DELEGATE-13)).
The entry gives how long the delegate ran, how many of its rounds it made out of the most its spec
allows, and one cause from a fixed set: it answered; it reached its round limit and answered with
what it had; it was stopped before it answered; it did not finish, with the fixed name of the
failure (`unavailable`, `refused`, `transport` and the rest); or it ended without handing anything
back, so its time and rounds are not known.

```
ok      delegate: d2: ended after 1559.0s and 37 of 120 rounds: it did not finish (unavailable)
```

The cause comes from which way the result came back, the run's round count and the failure's
category. None of it is text a service or a tool produced, so the entry holds no content (TRACE-2).
The note the person sees when the delegate finishes names the same cause. The planner is still told
only that the delegate did not finish ([BACKEND-37](backends.md#BACKEND-37)).

**Why.** Without it, a delegate that ran for 26 minutes and then did not finish leaves the reader of
the trail unable to tell whether it ran out of rounds, lost its backend or was stopped. Each calls
for a different response: raise the bound, retry, or nothing.

`verified-by: bravebot_core::policy::a_delegates_end_is_recorded_with_its_time_its_rounds_and_why`
`verified-by: bravebot_agent::turn::a_delegate_that_failed_leaves_its_fixed_cause_in_the_trail_and_none_of_the_reply`
`verified-by: bravebot_agent::turn::a_delegate_that_reached_its_round_limit_says_so_in_the_trail_and_the_note`
`verified-by: bravebot_agent::turn::a_stopped_delegate_and_a_lost_one_are_each_recorded_as_what_they_were`

<a id="TRACE-9"></a>
### TRACE-9: `/request` shows the request already built, each span with where its words came from

`/request`, typed by a person ([CMD-1](commands.md#CMD-1)), opens a read-only view of the last
request a turn built for the planner: the system prompt in the pieces it was put together from
(the instruction files, the skill list, the settings and the driver's own paragraphs), each message,
and each tool result as the planner saw it. Each span names its origin in one label: `typed`,
`trusted file <path>`, `tool result (trusted)`, `ref:N` for content the planner was not shown,
`driver` for a sentence this program wrote, `planner` for the model's own words sent back,
`setting` for a setting, `summary` for a compaction summary, `vetted ref:N` and `released: <where>` for
content let through from a reference, `typed, with dropped files` for a line sent with files, and
`unrecorded` where nothing recorded it. Other results are named by what they are, as in
`shell output (trusted)`.

The view also names, above the spans, every instruction file the system prompt was built from, each
with the bytes it contributed, and says none loaded where there were none. The list is read back out
of the system prompt's own `trusted file <path>` spans, so it cannot name a file whose words are not
in the request, and it holds only those: a trusted file sent with a prompt is a message rather than
an instruction, and the skill list and the driver's paragraphs are not files. The path is the one
the composing code recorded when it put the file into the prompt
([INSTR-5](instructions.md#INSTR-5)) and the length is of what was sent, counted and never read, so
no branch is taken on the bytes of a file.

The view is read from the request value handed to the backend, in memory, and never rebuilt from
the transcript or the saved record. The pieces of the system prompt are the text sent, joined, and
a prompt that cannot be shown in pieces that join to what was sent is shown whole as `unrecorded`.
A label is what the composing code recorded when it wrote the message, and is not worked out from
the words, so no branch is taken on the bytes of a span. A quarantined result or file is a `ref:N`
span holding the sentence the planner was given in its place; its bytes are not in the request and
so cannot be in the view. A delegate's requests are not shown. The view writes nothing to disk, the
key that exports the transcript is inert in it, and an incognito session is unchanged
([INCOG-1](incognito.md#INCOG-1)). It reuses the scroller of [watching.md](watching.md) and
closing it puts the transcript back where it was. Before any turn has sent a request the command
says so.

**Why.** What the planner was sent is the question behind every "why did it do that", and a view
reconstructed from the transcript would answer it with what the transcript says was sent. The
label is a claim about authority, so it comes from where the text was composed.

`verified-by: bravebot_agent::turn::the_view_of_the_request_labels_a_reference_by_its_token_and_holds_no_quarantined_body`
`verified-by: bravebot_agent::turn::the_view_of_the_request_labels_a_hidden_listing_by_its_references`
`verified-by: bravebot_agent::turn::the_view_of_the_request_names_a_trusted_result_and_a_trusted_file`
`verified-by: bravebot_agent::turn::the_view_of_the_request_labels_instruction_files_and_the_goal_and_is_the_system_prompt_sent`
`verified-by: bravebot_agent::turn::the_view_of_the_request_holds_no_instruction_file_the_kernel_withheld`
`verified-by: bravebot_agent::turn::the_view_of_the_request_does_not_call_a_prompt_the_driver_wrote_typed`
`verified-by: bravebot_agent::turn::no_view_of_the_request_is_built_for_a_reporter_that_does_not_ask`
`verified-by: bravebot_agent::turn::the_view_of_the_request_does_not_call_a_dropped_file_typed`
`verified-by: bravebot_agent::turn::the_view_of_the_request_does_not_call_released_output_trusted`
`verified-by: bravebot_agent::turn::the_view_of_the_request_is_the_parents_when_a_delegate_ran_and_labels_its_report`
`verified-by: bravebot_tui::state::clearing_the_conversation_forgets_the_request_it_sent`
`verified-by: bravebot_agent::conversation::a_message_is_labelled_by_what_was_recorded_and_never_guessed_at`
`verified-by: bravebot_agent::request_view::pieces_that_are_not_the_prompt_sent_are_not_trusted_to_describe_it`
`verified-by: bravebot_agent::request_view::the_instruction_files_are_the_trusted_files_the_system_prompt_was_built_from`
`verified-by: bravebot_agent::request_view::a_prompt_with_no_instruction_file_names_none`
`verified-by: bravebot_tui::render::the_request_view_draws_each_span_under_its_provenance`
`verified-by: bravebot_tui::render::the_request_view_names_the_instruction_files_loaded_with_their_lengths`
`verified-by: bravebot_tui::render::the_request_view_says_so_when_no_instruction_file_loaded`
`verified-by: bravebot_tui::state::the_request_view_opens_on_the_last_request_and_closing_restores_the_transcript`
`verified-by: bravebot_tui::app::the_key_that_writes_the_transcript_out_does_nothing_in_the_request_view`

## Known costs

- **A message restored from a saved session is `unrecorded`.** The origin is kept in memory beside
  the message and is not part of the saved record, so after `--resume` the earlier messages carry no
  label until the session sends new ones. Guessing one from the words would let a file's own bytes
  choose their label.
- **A prompt sent with dropped pictures or PDFs is one `typed, with dropped files` span.** The
  line and the files go in one message, so the label says the bytes in it are not all typed.
- **The view is of the planner's request, not of a delegate's, a summariser's or an advisor's.**

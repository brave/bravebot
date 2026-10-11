---
id: RPCVIEW
title: Shared session view
status: normative
governs:
  - crates/ui-bridge/src/view.rs
  - crates/ui-bridge/src/emit.rs
  - crates/ui-bridge/src/bridge.rs
  - crates/ui-bridge/src/running.rs
  - crates/ui-bridge/src/turn.rs
  - crates/ui-bridge/src/wire.rs
  - crates/ui-bridge/src/manifest.rs
  - crates/ui-bridge/src/control_link.rs
  - crates/sandbox/src/inspection.rs
  - packages/agent-client/src/common/wire.ts
  - packages/agent-client/src/common/interface.ts
  - packages/agent-client/src/common/view.ts
  - packages/agent-client/src/common/client.ts
documented-by: none (internal: the local RPC view is documented in ui/docs/phase-0-rpc-protocol.md for client authors)
---

## Scope

The opt-in Rust display view for fresh local bridge sessions, and the standalone TypeScript client
that applies it over stdio. The view has no listener, native binding, recovery, or controller
model. The view does not grant authority. Exact action targets (RPCVIEW-6) apply to every client of
the bridge, and RPCVIEW-8 states what the local channel they arrive on is. A client that names no
turn in a cancel stops whatever is running.

## Clauses

<a id="RPCVIEW-1"></a>
### RPCVIEW-1: a client explicitly starts a supported view

`agent.info` and `agent.ready` advertise `capabilities.sessionView`: version 1, the
`session.view.start` operation, `fresh_session` scope, the four supported approval kinds
(`confirm`, `run`, `fetch`, `ask`), and `reconnect: false`. A client requiring the view checks this
field and refuses an absent or unsupported version; it must not infer the view from legacy events.

`session.view.start` requires a session handle and version 1. It works once, before a fresh
session's first turn. An unsupported version, repeated start, running session, saved resume, or
fork is refused. It emits `session.view.initial` before its successful response. The initial view
has sequence 0, turn 0, no rows or pending turn approval, and `idle` or `awaiting_trust` status.
Startup trust still uses the existing explicit local `trust.reply` operation; starting a view
never answers it. Manifest runs are unsupported on sessions with this view and are refused.

Legacy clients that do not start a view receive no view events. Their existing operations,
responses and event payloads retain their meaning.

`verified-by: bravebot_ui_bridge::fetch::a_session_view_cannot_start_during_a_turn`
`verified-by: bravebot_ui_bridge::fetch::a_session_view_cannot_start_after_completion_resume_or_fork`
`verified-by: bravebot_ui_bridge::fetch::the_session_view_orders_prompts_approvals_and_labelled_results`
`verified-by: bravebot_ui_bridge::fetch::session_views_keep_two_sessions_and_terminal_outcomes_separate`
`verified-by: bravebot_ui_bridge::fetch::a_legacy_session_emits_no_view_events`

<a id="RPCVIEW-2"></a>
### RPCVIEW-2: Rust supplies ordered rows and authoritative status

Each `session.view.update` carries a consecutive per-session sequence number, current turn number,
status, pending approval or null, and replacement rows. Each row has a monotonically allocated
session-local ID, turn number, typed kind, original event name or null for a prompt, payload and
`resolved` flag. Replacements retain their ID and position. Clients replace the supplied rows and
status fields; they do not derive busy state or approval state from text or legacy events.

The connection lifetime, session handle, turn, and row ID identify a displayed item. These are
not durable identities. A request ID is unique within its session (RPCVIEW-6); the view adds no
stale-action protection beyond the targets that clause describes.

Accepted prompts produce rows before worker events. Rejected sends produce none. Composed prompts
use the same tag-only projection as saved history. Narration, quarantined previews, tool starts
and finishes, approvals, and terminal replies/errors produce rows. Other legacy events, including
audit, token counts, todos and phases, are outside version 1's transcript.

Status is `awaiting_trust`, `idle`, `running`, `waiting`, `completed`, `failed`, `cancelled`, or
`detached`. Terminal updates clear pending approvals. Completion publication and next-turn
eligibility share the emitter lock, so the next turn cannot publish ahead of the terminal update.
Closing a session detaches and removes its view; `detached` says nothing about worker termination,
save success, or rollback. EOF retains the existing process-ending behavior.

The bridge retains only current view metadata and the pending payload. Clients retain transcript
rows. There is no history snapshot, late subscription, reconnect, or resynchronization operation.
Connection loss or a sequence gap ends the view; a client must not claim recovered history.

`verified-by: bravebot_ui_bridge::fetch::the_session_view_orders_prompts_approvals_and_labelled_results`
`verified-by: bravebot_ui_bridge::fetch::session_views_keep_two_sessions_and_terminal_outcomes_separate`
`verified-by: bravebot_ui_bridge::emit::completion_is_eligible_before_publication_and_precedes_the_next_turn`
`verified-by: bravebot_ui_bridge::view::failures_have_authoritative_terminal_status`
`verified-by: bravebot_ui_bridge::wire::accepted_composed_prompts_cross_without_their_text`

<a id="RPCVIEW-3"></a>
### RPCVIEW-3: the existing approval implementation owns answers

The view reports a question's row ID, request ID, kind, supported flag and full existing wire
payload. Only `confirm`, `run`, `fetch` and `ask` are marked supported. Other question kinds remain
visible as unsupported; a client must use a capable local surface or cancel, never invent an
answer. This capability describes display support, not a remote authorization allowlist.

`Running` and `BridgeConfirmer` retain their ID/kind checks and single-use answers. Question
publication completes before an answer can consume it. A consumed question's resolved replacement
is published before waking the worker; cancellation also resolves the row. The resolved row keeps
its original payload and question kind. `resolved` means no answer is pending, not that the effect
was approved or succeeded. Wrong-kind and duplicate answers grant nothing.

`verified-by: bravebot_ui_bridge::view::approval_replacements_preserve_the_payload_and_kind`
`verified-by: bravebot_ui_bridge::fetch::the_session_view_orders_prompts_approvals_and_labelled_results`
`verified-by: bravebot_ui_bridge::fetch::session_views_keep_two_sessions_and_terminal_outcomes_separate`
`verified-by: bravebot_ui_bridge::fetch::a_yes_to_another_kind_of_question_does_not_send_a_fetch`

<a id="RPCVIEW-4"></a>
### RPCVIEW-4: display projection carries released payloads opaquely

The view copies complete released payloads, including labels, origin, reach, omissions and approval
fields. It selects transitions only from bridge-owned event names and identity/status metadata.
It neither inspects released text nor supplies display data to execution or planner input.

`view.rs` belongs beside the bridge under the [layering spec](layering.md)'s non-presentation constraint. Version 1 performs
no control-character or label-to-segment transform. A future content-reading transform belongs in
an allowed presentation module or crate with its first caller; placing it in this projection is
forbidden. A rendering surface must still meet the [layering spec](layering.md)'s marking rules. The package's
terminal program escapes control, zero-width and bidirectional characters in its own diagnostic dump; it is not a
rendering surface and does not trigger extraction.

`verified-by: bravebot_ui_bridge::fetch::the_session_view_orders_prompts_approvals_and_labelled_results`
`verified-by: bravebot_ui_bridge::view::approval_replacements_preserve_the_payload_and_kind`

<a id="RPCVIEW-6"></a>
### RPCVIEW-6: an old answer or cancel cannot act on a later turn

A question's number is allocated from a counter that belongs to its session and is never reused,
so an answer meant for one question cannot match a later question in any turn or run of that
session. A session whose counter is spent asks no more questions, and a question not asked is
refused.

A turn or manifest run is stopped by the target it was started with, and a target is never handed
out twice. `turn.send` and `manifest.run` return it, and the session view carries the target of the
turn on screen. `turn.cancel` may name a target. A cancel that names a target other than the one
running, or one that has ended, stops nothing and answers `{ "cancelled": false }`; one that names
the running target answers `{ "cancelled": true }`. A cancel that names no target stops whatever is
running and answers `{}`. A `target` that is not a number is refused. A `turn` parameter is refused, because a cancel that ignored it would stop whatever is running. A target is unique within one bridge process; a restarted bridge counts again. A turn number could not do
this job: it repeats after `session.rewind`, so a delayed cancel for the old turn 3 would stop the
new turn 3, and a manifest run has no turn number of its own. A session with a view cannot start a run, so the view never has to carry a run's target; a bridge that lets one do so must report the run's target in the view.

`trust.reply` is taken once, while the session's startup question is waiting. A repeat, and any
answer to a session that was never asked, is refused with `no_such_request` before the session's
state is touched, so it neither replaces the trust already given nor waits behind a running turn.

`agent.info` and `agent.ready` advertise this as `capabilities.actionTargets`, version 1, with
`questionIds: session`, `cancel: expected_target` and `trust: once`. A bridge that does not
advertise it numbers questions per turn, takes a cancel for whatever is running, and accepts a
repeated trust answer, and a client talking to one must not claim the stronger protection.

`verified-by: bravebot_ui_bridge::targets_tests::a_cancel_naming_another_target_stops_nothing`
`verified-by: bravebot_ui_bridge::targets_tests::a_cancel_naming_the_running_target_stops_it`
`verified-by: bravebot_ui_bridge::targets_tests::a_cancel_naming_a_finished_target_stops_nothing`
`verified-by: bravebot_ui_bridge::targets_tests::a_cancel_naming_nothing_stops_whatever_is_running`
`verified-by: bravebot_ui_bridge::targets_tests::a_target_is_never_handed_out_twice`
`verified-by: bravebot_ui_bridge::targets_tests::a_target_that_is_not_a_number_is_refused_and_stops_nothing`
`verified-by: bravebot_ui_bridge::targets_tests::a_cancel_naming_a_turn_number_is_refused_and_stops_nothing`
`verified-by: bravebot_ui_bridge::rewind::a_turn_after_a_rewind_has_a_new_target_though_its_number_repeats`
`verified-by: bravebot_ui_bridge::targets_tests::a_repeated_trust_reply_does_not_wait_behind_a_running_turn`
`verified-by: bravebot_ui_bridge::manifest::a_run_is_stopped_by_its_own_target_and_by_no_other`
`verified-by: bravebot_ui_bridge::manifest::a_session_with_a_view_cannot_start_a_run`
`verified-by: bravebot_ui_bridge::targets_tests::the_bridge_advertises_what_it_promises_about_targets`
`verified-by: bravebot_ui_bridge::refusal::question_numbers_are_not_reused_by_a_later_turn_of_the_same_session`
`verified-by: bravebot_ui_bridge::refusal::a_session_with_no_question_numbers_left_refuses_to_ask`
`verified-by: bravebot_ui_bridge::fetch::a_later_turn_does_not_reuse_an_earlier_questions_number`
`verified-by: bravebot_ui_bridge::remembered_trust::a_repeated_trust_answer_is_refused_and_the_first_stands`
`verified-by: bravebot_ui_bridge::remembered_trust::a_trust_answer_cannot_be_changed_after_a_turn_has_used_it`

<a id="RPCVIEW-5"></a>
### RPCVIEW-5: the TypeScript client applies the view and decides nothing

`packages/agent-client` is the stdio client. Its wire types are checked against the Rust types
through a contract file that a Rust test writes and compares. The client requires the advertised
capability and refuses a runtime that lacks version 1, before it creates a session. It starts the
view itself and never builds one from legacy events.

It applies each update by replacing the listed rows at their positions and replacing every status
field. A sequence that is not the next one, a malformed update, or the end of the connection ends
that view at its last received state; it claims no recovery. It reports a close as a detached view
with worker termination and save success unknown. A failed close keeps the session registered for
view updates and connection-loss reporting, unless the bridge reports that the session is gone. Sessions open only in configured workspace ids, and a
request unanswered past its deadline ends the connection with its outcome unknown. A failed request
reports whether it was rejected, meaning refused with no effect, or unknown, meaning it may have
reached the bridge: a lost connection, a failed write, a deadline, or an internal bridge error. At
most 256 requests wait for an answer, with cancel and close exempt so a caller can always stop work,
and one request is at most 8 MiB; a request beyond either limit is refused before it is written.
Startup trust is sent only when a caller asks. No export of the package reaches raw dispatch or the connection, so these
rules hold for every caller. The client holds released payloads as received and branches on none of
them. Rust still decides every transition and every answer. Replies go to the displayed question:
the method comes from the pending kind Rust supplied, a stale request is refused without sending, a
question the client cannot answer is refused without being sent, and a run is answered once without
`remember`.

`verified-by: bravebot_ui_bridge::view::the_client_wire_contract_matches_the_rust_types`
`verified-by: by-construction (packages/agent-client/tests runs the shared scenarios under four chunkings, and drives a real bravebot-rpc against a model service of its own for accepted turns, trust deciding whether a write is asked about, approval and rejection of a write, a command and a fetch with different real effects, a typed and a declined answer to a question, cancellation, interleaved sessions, fetched bytes copied into a file, a write that lands when the session cannot be saved, close, EOF, malformed input and failing callbacks, with labelled rows carried whole in the scenarios, and runs the local program in scripts/local-client.ts against the same process; make check-agent-client runs it, and the Front end CI job runs that target)`

<a id="RPCVIEW-7"></a>
### RPCVIEW-7: the client names what Stop is for from values Rust reported

`AgentSession.cancel(target?)` names a target only when the runtime advertises
`cancel: expected_target`. The target is the one the caller gives, which is the `target` a send
returned, or else the target of the turn on screen. The only choice the client makes is which
reported value to send; it derives no target of its own. It names none, and so stops whatever is
running as Stop always has, in three cases: the runtime does not advertise it, a send has not been
answered so the target of the turn it starts is not yet known, or the view has ended and shows a
stale turn. A target the caller gives is never dropped: against a runtime that does not advertise it,
the cancel is refused with `UnsupportedError` and nothing is sent, since the runtime would stop
whatever is running instead. The result is `cancelled: true` or `false` as the bridge answered when a target was
named, and `null` when none was.

`verified-by: by-construction (packages/agent-client/test-fixtures/scenarios/cancel-names-the-target-on-screen.json, cancel-names-the-target-it-is-given.json, cancel-names-the-new-target-when-turn-numbers-go-back.json, cancel-while-a-send-is-unanswered-names-no-target.json and cancel-without-action-targets-names-no-target.json and send-with-a-target-that-is-not-a-count-is-refused.json run under four chunkings, and tests/real-rpc.test.ts drives a real bravebot-rpc through a delayed cancel for an earlier turn, a cancel after a rewind that numbers a turn again, and a cancel of the turn just sent)`

<a id="RPCVIEW-8"></a>
### RPCVIEW-8: the control channel has no name a tool could use

On Unix, a program that speaks to the bridge, or that a host launches to speak for a person, is started over an anonymous socket pair that the launcher creates (`control_link::launch`; Node's child processes make the same kind of pair for the existing clients). The launched program receives one end as its standard input and output, and the launcher keeps the other and closes its own copy of the first. No socket path, port, token or file names the channel. A program running as the same user, such as an approved command, a hook, or a process either of them left running after its parent exited, therefore has nothing to connect to, and nothing here tells a client from a tool by its user, its parent or its directory, because none of those differ.

The launcher takes the program and its arguments from the person's own configuration, never from a tool result, a workspace file or a program a tool can replace. A client that exits ends its channel, and a replacement is a new launch with a new pair chosen by the host. A program that was not launched this way cannot attach to a running host. `control_link::launch` is the Rust launcher for the persistent host, and its module is hidden from the crate's documentation. The Electron and TypeScript clients start the bridge through Node's child processes, which make the same kind of pair.

The bridge starts hooks, the `run` tool, shell-mode commands, language servers, the AWS CLI and its probes with null, piped or file standard streams, so none hands a program it starts the channel. A descriptor of the channel that reached a tool would let it read what the person sends or put lines into what the person reads, and is a defect.

Three limits belong to the launcher and the bridge. First, creating a socket pair is not one atomic step on every system: macOS marks the descriptors close-on-exec in a second step, so a process another thread starts in between inherits them and the bridge hands them on to its tools. The creation of a channel (`bravebot_sandbox::process::creating_a_channel`) and every start of a program by a crate the bridge links (`starting_a_program`) therefore take one gate, one at a time: a start creates the pipes of its own standard streams inside `spawn`, so two starts overlapping could leave one holding the other's pipe ends. The gate is held until the program has started, because the standard library offers no narrower point. It is not reentrant, and entering it again panics. The pipes the agent makes for a stage's standard streams are made under the same gate. A host that starts programs another way takes it too. A descriptor the process was started with and that is not close-on-exec, such as a socket a CI runner leaves open, reaches every program it starts. Descriptors made in two steps outside the gate, such as the TCP and TLS sockets to a model endpoint, are not covered: a program started in the window can inherit one, and because the non-dumpable mark does not survive `execve`, a tool holding a leaked descriptor can be inspected. Second, on Linux a pipe can be reopened by path through `/proc/<pid>/fd` by the same user and a socket cannot, so a launcher uses a socket pair, as the child processes of Node do. Third, on Linux a process of the same user can copy a descriptor out of the bridge with `pidfd_getfd`, or attach with `ptrace`, when the kernel's ptrace policy allows it, so each binary that speaks over its own standard input and output (`bravebot-rpc` and `bravebot-acp`) marks itself non-dumpable at startup (`inspection::refuse_same_user_inspection`) and says on standard error when that fails. The protection takes effect when `main` begins, so a process already positioned to act before that is outside the claim. A host that enables control of real sessions reads that outcome and treats a failure to set the mark on Linux as the channel not being safe. The mark also stops the developer who started the bridge from attaching `strace` or `gdb` to it and makes its `/proc/<pid>` entries belong to root; a debugger has to be started with the bridge, or run with `CAP_SYS_PTRACE`. A debugger on macOS, or a process with the same user's rights to read another process's memory by other means, is outside this clause. Windows is not covered. The `pidfd_getfd` attack counts only where it also works against a process of the same user that did not refuse it and that, like the bridge, is not a descendant of the attacker. Elsewhere the test says the attack was not exercised, and passes.

`verified-by: bravebot_ui_bridge::control_boundary::a_program_a_hook_leaves_running_cannot_reach_the_channel`
`verified-by: bravebot_ui_bridge::control_boundary::a_program_an_approved_command_leaves_running_cannot_reach_the_channel`
`verified-by: bravebot_ui_bridge::control_boundary::the_channel_has_no_name_and_each_launch_gets_its_own`
`verified-by: bravebot_ui_bridge::control_boundary::the_channel_ends_when_the_client_exits_and_a_replacement_gets_a_new_one`
`verified-by: bravebot_ui_bridge::control_boundary::a_program_started_afterwards_inherits_no_part_of_the_channel`
`verified-by: bravebot_ui_bridge::control_boundary::a_launch_waits_for_a_program_being_started`
`verified-by: bravebot_ui_bridge::control_boundary::programs_started_while_channels_are_created_hold_no_socket`
`verified-by: bravebot_ui_bridge::control_boundary::programs_a_host_starts_hold_no_end_of_the_channels_it_created`
`verified-by: bravebot_agent::spawn_gate::a_hook_waits_for_a_channel_being_created`
`verified-by: bravebot_agent::spawn_gate::a_run_stage_waits_for_a_channel_being_created`
`verified-by: bravebot_agent::spawn_gate::a_shell_mode_command_waits_for_a_channel_being_created`
`verified-by: bravebot_sandbox::process::a_start_waits_for_a_channel_being_created`
`verified-by: bravebot_sandbox::inspection::the_kernel_marks_the_process_non_dumpable`
`verified-by: bravebot_ui_bridge::layering::every_binary_that_speaks_over_its_own_stdio_marks_itself_non_dumpable`
`verified-by: bravebot_ui_bridge::layering::every_program_the_bridge_process_starts_goes_through_the_spawn_gate`
`verified-by: bravebot_agent::spawn_gate::a_usability_stage_waits_for_a_channel_being_created`
`verified-by: bravebot_agent::spawn_gate::the_login_probe_waits_for_a_channel_being_created`
`verified-by: bravebot_ui_bridge::control_boundary::copying_nothing_out_of_the_bridge_proves_nothing_where_the_attack_never_worked`
`verified-by: bravebot_sandbox::inspection::a_failed_remedy_is_said_with_its_error`

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
documented-by: none (internal: the local RPC view is documented in ui/docs/phase-0-rpc-protocol.md for client authors)
---

## Scope

The opt-in Rust display view for fresh local bridge sessions. The view has no client package,
listener, native binding, recovery, or controller model. The view does not grant authority or
change the existing reply and cancellation targets.

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
not durable identities. Request IDs retain their existing per-turn meaning; the view adds no
stale-action protection to legacy reply operations.

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
forbidden. A rendering surface must still meet the [layering spec](layering.md)'s marking rules.

`verified-by: bravebot_ui_bridge::fetch::the_session_view_orders_prompts_approvals_and_labelled_results`
`verified-by: bravebot_ui_bridge::view::approval_replacements_preserve_the_payload_and_kind`

---
id: ACP
title: Hosting a session in an editor over the Agent Client Protocol
status: normative
governs:
  - crates/ui-bridge/src/acp.rs
  - crates/ui-bridge/src/bin/bravebot-acp.rs
documented-by: docs/website/docs/using/editors.md
---

## Scope

`bravebot-acp` is a program an editor starts and speaks the Agent Client Protocol to on its
standard input and output, as newline-delimited JSON-RPC 2.0. The editor draws the conversation and
the agent runs it. The program is a projection of the session the desktop window uses, so the turn
engine, the trust map and every question are the ones described in [prompting.md](prompting.md),
[trust-map.md](trust-map.md) and [layering.md](layering.md), and this spec decides only what the
protocol adds: where each message comes from, and what an answer is allowed to grant.

The agent advertises `loadSession: false`, no authentication methods, and prompt capabilities for
text, links and images. It does not call the editor's file or terminal methods, so bytes an editor
holds reach the agent only as a prompt or an attachment.

## Clauses

<a id="ACP-1"></a>
### ACP-1: a prompt is a line a person typed

The text blocks of a `session/prompt` are the words of a prompt the person typed, on the footing
a typed line has. A slash word in them is text for the model: no command runs, because a command is
a line typed at an interface that takes commands, and this one does not.

`verified-by: bravebot_ui_bridge::acp::a_slash_word_in_a_prompt_is_text`

<a id="ACP-2"></a>
### ACP-2: what an editor attaches is carried as a paste or a drop is

An image block is carried as a pasted picture and a link to a `file:` URI is carried as a dropped
file, both on the footing those gestures have ([pasting.md](pasting.md),
[dropping.md](dropping.md)). The driver never reads the bytes: the agent reads a dropped file into
a message of its own, and nothing from it enters the words the editor sent. A link to anything but
a local file, a link to a file on another host, an embedded resource and audio are refused with
invalid parameters before a turn starts, because the only way to carry them would be to put their
bytes in the words. A list of MCP servers in `session/new` is not acted on, since a server is
declared in a person's own directory and asked about before it starts.

`verified-by: bravebot_ui_bridge::acp::what_an_editor_attaches_is_carried_as_a_paste_and_a_drop_are`
`verified-by: bravebot_ui_bridge::acp::a_block_that_could_only_be_carried_as_words_is_refused`

<a id="ACP-3"></a>
### ACP-3: a question is put as a permission request, and only a selected approval approves

Every question a turn raises that is a permission is sent as `session/request_permission`, carrying
what the drawn prompt shows: the call, its target, the change or the command, and the raw request.
The options are the single allow and the refusal, and a standing allow only where the question
offers something to record. An answer approves only when it selects an approving option the question
offered. A cancelled outcome, an error from a client that cannot answer, a refusing option, an
option never offered and a selection with no option all refuse. A client that selects the standing
option for a question that offered none gets the single allow, which is among the options it was
shown.

**Why.** An approval is the one thing that must never be produced by accident, so an answer that is
unreadable has the same effect as no answer.

`verified-by: bravebot_ui_bridge::acp::a_write_the_editor_approves_lands`
`verified-by: bravebot_ui_bridge::acp::a_client_that_cannot_answer_a_permission_request_refuses_it`
`verified-by: bravebot_ui_bridge::acp::a_cancelled_or_refusing_or_unlisted_answer_is_a_refusal`
`verified-by: bravebot_ui_bridge::acp::a_standing_answer_to_a_question_that_offers_none_is_the_single_yes`

<a id="ACP-4"></a>
### ACP-4: a question nobody answers is refused, and stopping a prompt refuses it

A turn waiting on a permission request waits for the editor. `session/cancel` stops the turn, the
question is refused with no effect, and the prompt ends with the stop reason `cancelled`. The end of
the input refuses every question still waiting, so a write that was waiting does not happen.

`verified-by: bravebot_ui_bridge::acp::cancelling_a_prompt_that_waits_on_a_question_refuses_the_question`
`verified-by: bravebot_ui_bridge::acp::the_end_of_the_input_refuses_a_question_still_waiting`

<a id="ACP-5"></a>
### ACP-5: the trust question comes first, and the planner's own questions are not permissions

The first prompt of a session whose directory has no recorded answer puts the trust question to the
editor as a permission request, and no turn starts and no model is asked until it is answered. A
refusal is an answer: the session runs with no path trusted. The question is put once per session
and an answer is never remembered. Stopping the prompt at that question answers it no, so the session still takes a prompt afterwards. A question of the planner's own is not a permission, so it is
not put to the editor; the turn continues as it does where nobody could be asked.

`verified-by: bravebot_ui_bridge::acp::no_turn_starts_before_the_trust_question_is_answered`
`verified-by: bravebot_ui_bridge::acp::a_session_asks_about_trust_once`
`verified-by: bravebot_ui_bridge::acp::a_session_stopped_at_the_trust_question_can_still_take_a_prompt`
`verified-by: bravebot_ui_bridge::acp::a_question_of_the_planners_is_not_put_to_the_editor_as_a_permission`

<a id="ACP-6"></a>
### ACP-6: the standard output holds protocol messages and nothing a model said as one

Everything written to standard output is one JSON-RPC message on one line. Text a model or a tool
produced reaches the editor only as a string inside a notification, whatever it looks like, and
nothing read from a model or a tool is ever interpreted as a message of the protocol. Only the
messages an editor writes are read as requests, notifications and answers.

`verified-by: bravebot_ui_bridge::acp::a_reply_that_looks_like_a_protocol_message_is_only_text`
`verified-by: bravebot_ui_bridge::acp::the_binary_writes_only_protocol_messages_to_stdout`
`verified-by: bravebot_ui_bridge::acp::the_agent_advertises_what_it_carries_and_nothing_more`

<a id="ACP-7"></a>
### ACP-7: an editor may select asking, accepting edits or planning, and never bypassing

`session/new` offers three modes and `session/set_mode` accepts those three: `ask`, `acceptEdits`
and `plan`. A request naming `bypass`, or any other word, is refused with invalid parameters.
Bypassing is reachable only where the command line asked for it
([MODE-5](permission-modes.md#MODE-5)), and an editor has no command line, which is the reason
[MODE-11](permission-modes.md#MODE-11) gives for the desktop window. What each mode answers before a
question reaches the editor is [MODE-2](permission-modes.md#MODE-2) for accepting edits and
[MODE-3](permission-modes.md#MODE-3) for planning.

`verified-by: bravebot_ui_bridge::acp::an_editor_selects_three_modes_and_bypass_is_refused`

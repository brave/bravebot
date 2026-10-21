---
id: MCP
title: Model Context Protocol servers
status: normative
governs:
  - crates/mcp/src/lib.rs
  - crates/mcp/src/stdio.rs
  - crates/mcp/src/http.rs
  - crates/mcp/src/protocol.rs
documented-by: docs/website/docs/customize/mcp-servers.md
---

## Scope

Tools that come from outside this repository, over stdio or HTTP, and the prompts such a server
offers a person. What such a server returns, what it is allowed to do, and why the tools bravebot
ships are not built this way. Confinement of a stdio server is [sandboxing.md](sandboxing.md). How
a person runs a prompt is [commands.md](commands.md).

## Why the built-in tools stay native

An MCP call is opaque: the whole call goes out and a result comes back. That erases the split
between the part of a call that decides where it lands and the part that is merely carried, which
the built-in tools depend on, since a path is one and a file's contents are the other. A primitive
that needs its parts labelled separately stays native rather than moving behind this boundary.

## Clauses

<a id="MCP-1"></a>
### MCP-1: what a server returns is untrusted and private

A tool result is content from outside, so it is labelled untrusted and quarantined like anything
else nobody vouched for. It is labelled private too, so a line fed one is put to the person as a
line fed a file is ([PERM-9](permissions.md#PERM-9)). Nothing a server says about itself changes
either label, a result it marks as its own failure included. The words a server describes its tools
with stay public ([SERVERS-8](mcp-servers.md#SERVERS-8)).

**Why private.** A server may hold the person's own data, a mailbox or a calendar, and this client
cannot tell what a given call read. Labelling a search server's answer private costs one question
when it is fed to a program. Labelling a mailbox's answer public would hand the person's mail to
that program unasked.

`verified-by: bravebot_mcp::stdio::a_tool_result_is_labelled_untrusted_and_private`
`verified-by: bravebot_mcp::http::a_tool_result_is_labelled_untrusted_and_private`
`verified-by: bravebot_mcp::stdio::a_tool_level_error_is_reported_as_a_failure`
`verified-by: bravebot_mcp::http::a_tool_level_error_is_reported_as_a_failure`
`verified-by: bravebot_agent::mcp::a_servers_answer_fed_to_a_vouched_line_is_still_put_to_a_person`

<a id="MCP-2"></a>
### MCP-2: a call needs the capability, like any other effect

A tool call without the capability granted is refused, so adding a server does not widen what the
agent may do.

`verified-by: bravebot_mcp::stdio::a_tool_call_without_the_capability_is_refused`

<a id="MCP-3"></a>
### MCP-3: a stdio server is not launched without confinement

If the process cannot be confined it is not started, rather than started unconfined.

**Why.** A stdio server runs code we did not write, which is precisely the case an
operating-system boundary exists for.

`verified-by: bravebot_mcp::stdio::a_server_is_not_launched_without_confinement`
`verified-by: bravebot_mcp::stdio::a_confined_server_completes_the_handshake_and_lists_tools`

<a id="MCP-4"></a>
### MCP-4: bravebot advertises no capabilities of its own to a server

The handshake offers nothing, so a server cannot ask this process to do anything on its behalf.

`verified-by: bravebot_mcp::protocol::initialize_advertises_no_capabilities`
`verified-by: bravebot_mcp::protocol::call_params_carry_the_name_and_arguments`

<a id="MCP-5"></a>
### MCP-5: a failure is reported, never treated as an empty result

A server error, a tool-level error, a server that exits early, and a request not answered within
its bound ([SERVERS-15](mcp-servers.md#SERVERS-15)) are all reported as failures. A reply carrying no
JSON is nothing rather than an empty success.

**Why.** An error read as "no results" would have the planner conclude a thing does not exist when
the truth is that nobody asked successfully.

`verified-by: bravebot_mcp::stdio::a_server_error_is_reported`
`verified-by: bravebot_mcp::stdio::a_server_that_exits_early_is_an_error`
`verified-by: bravebot_mcp::stdio::a_tool_level_error_is_reported_as_a_failure`
`verified-by: bravebot_mcp::http::a_tool_level_error_is_reported_as_a_failure`
`verified-by: bravebot_mcp::protocol::a_tool_level_error_is_visible`
`verified-by: bravebot_mcp::protocol::an_error_response_parses`
`verified-by: bravebot_mcp::http::a_reply_with_no_json_is_none`
`verified-by: bravebot_mcp::stdio::a_call_not_answered_within_its_bound_is_a_timeout_naming_the_tool_and_the_bound`

<a id="MCP-6"></a>
### MCP-6: only text content is taken from a result

Non-text content is ignored rather than guessed at, and several text parts are joined.

`verified-by: bravebot_mcp::protocol::tool_result_text_is_joined`
`verified-by: bravebot_mcp::protocol::non_text_content_is_ignored`
`verified-by: bravebot_mcp::protocol::a_block_of_another_type_is_ignored_even_when_it_carries_text`

<a id="MCP-7"></a>
### MCP-7: the transport is parsed strictly

A request carries the protocol version, a notification has no id, and an HTTP reply framed as
server-sent events is unwrapped with the last payload winning.

`verified-by: bravebot_mcp::protocol::a_request_carries_the_jsonrpc_version`
`verified-by: bravebot_mcp::protocol::a_notification_has_no_id`
`verified-by: bravebot_mcp::protocol::a_successful_response_parses`
`verified-by: bravebot_mcp::protocol::a_tool_list_parses_with_schemas`
`verified-by: bravebot_mcp::http::an_sse_framed_reply_is_unwrapped`
`verified-by: bravebot_mcp::http::the_last_sse_payload_wins`
`verified-by: bravebot_mcp::http::plain_json_is_extracted_as_is`
`verified-by: bravebot_mcp::http::whitespace_around_json_is_tolerated`
`verified-by: bravebot_mcp::http::a_server_records_its_configuration`

<a id="MCP-8"></a>
### MCP-8: a failure's own text is written here, and what a server wrote is shown to a person or not at all

The text of every failure a server can cause is structure this process chose or the protocol
numbered: the tool a call was made to, the method that was put, and the code a rejection carries.

A tool that ran and reported failure also carries the server's account of why, as labelled
content, and reading that account takes the release that puts content on a screen, so it reaches
the person watching and never a message the planner is sent. No other failure holds what a server
wrote at all. The `message` beside a JSON-RPC error is not deserialised, so there is no field
holding it. A reply this client will not parse is reported as what was being read and which of the
parser's four kinds of failure it was, not as the parser's own sentence, which quotes the value it
rejected.

**Why.** An error's text is the part of a failure a caller formats into whatever it is building,
including a message the planner is sent, which is the context a hostile server exists to reach. A
server composes `error.message` freely, with nothing in the protocol constraining it, and a
parser's sentence quotes bytes the server chose, so both are a server's prose wearing this
process's words. A tool's account is kept, under a label, because a tool failure is the one whose
explanation is usually a person's own server and there is a person on the screen to read it.

What a stdio server writes to its standard error is neither a failure nor a reply, and it goes
where the caller launching the server sends it: to the terminal a plain session or a one-shot run
writes to, where the person watching reads it, and nowhere under the full-screen interface, where it
would draw over the screen. It is never read by this process, so it reaches no message the planner
is sent. The two streams the protocol runs over are pipes whatever the caller chose.

**A known cost.** A server that fails for a reason only its own sentence gives reports a code and
a method and nothing else, and a reply this client rejects reports which kind of malformed it was
and not where. Carrying either under a label instead would put it behind the release a tool
result's account is behind, where nobody is watching a handshake or a tool listing. Under the
full-screen interface a server's own diagnostics are discarded as well, so a server that fails
there says only what its failure's code and method say.

`verified-by: bravebot_mcp::lib::a_failing_tools_detail_stays_out_of_the_error_message`
`verified-by: bravebot_mcp::lib::a_server_failure_reports_the_method_and_the_code`
`verified-by: bravebot_mcp::lib::a_rejected_reply_is_reported_as_a_kind_and_not_the_parsers_sentence`
`verified-by: bravebot_mcp::protocol::an_error_response_parses`
`verified-by: bravebot_mcp::stdio::a_tool_level_error_is_reported_as_a_failure`
`verified-by: bravebot_mcp::stdio::a_server_error_is_reported`
`verified-by: bravebot_mcp::http::a_tool_level_error_is_reported_as_a_failure`
`verified-by: bravebot_mcp::http::a_server_failure_names_the_method_and_not_the_servers_words`
`verified-by: bravebot_mcp::http::a_rejected_reply_names_what_was_read_and_not_the_servers_words`
`verified-by: bravebot_mcp::stdio::a_servers_diagnostics_go_where_its_caller_sent_them`

<a id="MCP-9"></a>
### MCP-9: a stdio server is started with none of this process's environment

The environment this process holds is emptied before a server starts, on every platform. What the
server receives is the variables its caller hands it and nothing else, which is the variables its
declaration names and a `HOME` naming a directory of the server's own
([SERVERS-10](mcp-servers.md#SERVERS-10)).

**Why.** A server is code we did not write, and a credential this process authenticates with sits
in a variable rather than in a file, so confinement over paths withholds none of it. A program
`run` starts keeps the rest of the environment because a person typed the line and it is meant to
behave as their own terminal does ([tools/run.md](tools/run.md)); nobody types a server, so there
is no such expectation to meet here. The launch asks for an empty environment rather than leaving
the answer to whichever backend confines the process, so both platforms hand a server the same
nothing.

A server that reads a variable to work at all names it in its declaration, and receives that one,
with the value this process holds for it at launch: a command resolved through `PATH` names `PATH`,
and a server wanting a token of a person's own names the variable holding it. That is
[SERVERS-10](mcp-servers.md#SERVERS-10), and it is why handing a server a variable is a caller's
statement of which ones rather than a switch that restores them all.

`verified-by: bravebot_mcp::stdio::a_server_does_not_receive_this_processes_environment`
`verified-by: bravebot_mcp::stdio::a_server_receives_the_variables_it_was_handed_and_no_others`

<a id="MCP-10"></a>
### MCP-10: a server's prompts are asked for only where it declared them, drawn for a person, and never offered to the planner

A server that said in its handshake that it has prompts is asked for them, a page at a time as its
tools are. The list is drawn as a tool list is. Each prompt is its word, its description cleaned of
every control character and every character that reorders or hides text and cut at the length a
tool's is, and its arguments by name with whether each is required, and nothing else the server
sent: what an argument's own description says is dropped. A prompt is refused, and counted without
being named, where its word is not one a tool may be named (letters, digits, `-` and `_`), where
its word is listed twice, and where its arguments cannot be read as a list of names. The list is
labelled untrusted and public.

The list is for a person's screen. Nothing of it is in the tools the planner is offered, in the
list a person is asked to vouch for ([mcp-servers.md](mcp-servers.md)), or in any message the
planner is sent.

A server that did not declare prompts is sent no request for them and offers none. The declaration
is an object, as the protocol writes it. Where one that declared them rejects the request with a
protocol error, it is kept for its tools with no prompts and the person is told, without what the
server wrote beside the error. Any other failure of the request is a failure of its handshake, as
a failure to list its tools is (MCP-5), since a request that ran out of time stopped the process.
A list that held entries the client refused says how many.

**Why.** A prompt's word and description are a server's words, and a menu is where a sentence
composed as an instruction is read as one by whoever is choosing. The word is one a tool may be
named for that reason, and the sentence is cleaned and cut. The declaration is what the protocol
has a server make before a client asks, and it is what keeps a request away from a server that has
no such method: a stdio server may never answer a method it does not have, and a request that is
not answered stops the process and takes the server's tools with it.

`verified-by: bravebot_mcp::stdio::a_server_that_declares_prompts_lists_them`
`verified-by: bravebot_mcp::stdio::a_server_that_declares_no_prompts_is_not_asked_for_any`
`verified-by: bravebot_mcp::http::a_prompt_text_is_labelled_untrusted_and_private`
`verified-by: bravebot_mcp::http::a_remote_server_that_declares_no_prompts_is_not_asked_for_any`
`verified-by: bravebot_mcp::protocol::a_prompt_list_is_content_and_is_not_printed`
`verified-by: bravebot_mcp::protocol::a_prompt_word_that_is_not_a_function_name_is_refused_and_counted`
`verified-by: bravebot_mcp::protocol::a_prompt_word_listed_twice_is_refused_both_times`
`verified-by: bravebot_mcp::protocol::a_prompts_arguments_are_drawn_as_names_and_whether_they_are_required`
`verified-by: bravebot_mcp::protocol::a_prompt_list_in_pages_is_read_whole`
`verified-by: bravebot_mcp::protocol::a_server_declares_prompts_in_its_handshake`
`verified-by: bravebot_agent::servers::a_remote_server_that_declares_prompts_is_started_with_them_and_the_refused_are_counted`
`verified-by: bravebot_agent::servers::a_remote_server_that_rejects_its_prompt_list_is_kept_for_its_tools_and_the_person_is_told`
`verified-by: bravebot_agent::servers::a_local_server_that_declares_prompts_is_started_with_them`
`verified-by: bravebot_agent::mcp::a_prompt_list_reaches_a_screen_and_never_the_planner`

<a id="MCP-11"></a>
### MCP-11: a prompt a person runs is fetched with the arguments they typed, and its text is untrusted and private

A prompt is fetched when a person runs it ([commands.md](commands.md)). The request carries its
name and the arguments the person typed, as strings, and nothing a server listed: the name and the
arguments are the person's words and the list decides neither. The call needs the capability a
tool call needs (MCP-2), and a `deny` rule that covers the server and the word refuses it before
anything is sent, as it refuses a tool call.

The text of the messages is labelled as a tool's result is (MCP-1): untrusted, and private, because
what a server read to compose it may be the person's own. The role a message says it was written
as is not read, so a message a server dresses as the assistant's is the same text. Only text
content is taken and the messages are joined in order (MCP-6). A result with no text content, one
of pictures and embedded resources alone, is a failure and not an empty prompt (MCP-5). A rejection
is reported as the method and the code, and a reply this client will not parse as what was being
read (MCP-8).

Nobody is asked before the request. The line that runs the prompt is the person's own and holds the
arguments, which are what the server is sent. Choosing to run a prompt does not vouch for what it
says: the planner is given a reference to the text and never the text.

**Why.** A prompt's text is a server's output, and it is the kind of output written to be read as
instructions, so what it is labelled cannot depend on who asked for it. Asking the person again
would put a question in front of a line they had just sent, with nothing to add to what the line
says.

`verified-by: bravebot_mcp::stdio::a_prompt_text_is_labelled_untrusted_and_private`
`verified-by: bravebot_mcp::http::a_prompt_text_is_labelled_untrusted_and_private`
`verified-by: bravebot_mcp::stdio::getting_a_prompt_without_the_capability_is_refused`
`verified-by: bravebot_mcp::http::getting_a_remote_prompt_without_the_capability_is_refused`
`verified-by: bravebot_mcp::stdio::a_prompt_the_server_rejects_is_reported_as_the_method_and_the_code`
`verified-by: bravebot_mcp::protocol::prompt_text_is_joined_from_text_content_only`
`verified-by: bravebot_mcp::protocol::a_prompt_result_with_no_text_says_so`
`verified-by: bravebot_mcp::stdio::a_prompt_with_no_text_is_a_failure_and_not_an_empty_prompt`
`verified-by: bravebot_mcp::http::a_remote_prompt_with_no_text_is_a_failure_and_not_an_empty_prompt`
`verified-by: bravebot_mcp::protocol::get_params_carry_the_name_and_string_arguments`
`verified-by: bravebot_agent::mcp::a_prompt_a_person_ran_reaches_the_server_with_their_arguments_and_the_planner_gets_a_reference`
`verified-by: bravebot_agent::mcp::a_prompt_the_server_rejects_is_said_and_not_guessed_at`
`verified-by: bravebot_agent::mcp::a_rule_that_denies_a_name_refuses_the_prompt_before_anything_is_sent`

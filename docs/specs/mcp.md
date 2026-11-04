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
either label, a result it marks as its own failure included. A picture the result carries takes
the label its text takes ([MCP-6](#MCP-6)). What a session fetched with a tool of what a person
held out in a browser is a tool's result, and holding it out gives its bytes no label
([BROWSER-39](browser.md#BROWSER-39)). The words a server describes its tools with stay public
([SERVERS-8](mcp-servers.md#SERVERS-8)). A number carried into an argument from a choice
([MCP-14](#MCP-14)) is sent to the server, and what the server returns is labelled as before.

**Why private.** A server may hold the person's own data, a mailbox or a calendar, and this client
cannot tell what a given call read. Labelling a search server's answer private costs one question
when it is fed to a program. Labelling a mailbox's answer public would hand the person's mail to
that program unasked.

`verified-by: bravebot_mcp::stdio::a_tool_result_is_labelled_untrusted_and_private`
`verified-by: bravebot_mcp::http::a_tool_result_is_labelled_untrusted_and_private`
`verified-by: bravebot_mcp::stdio::a_tool_result_picture_is_labelled_untrusted_and_private`
`verified-by: bravebot_mcp::http::a_tool_result_picture_is_labelled_untrusted_and_private`
`verified-by: bravebot_mcp::stdio::a_tool_level_error_is_reported_as_a_failure`
`verified-by: bravebot_mcp::http::a_tool_level_error_is_reported_as_a_failure`
`verified-by: bravebot_agent::mcp::a_servers_answer_fed_to_a_vouched_line_is_still_put_to_a_person`
`verified-by: bravebot_agent::browser_group_servers::what_a_person_held_out_in_the_browser_is_a_reference_and_the_planner_never_holds_it`

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

A failure says whether a reply arrived, so that a caller can treat every reply alike whatever it
holds. A tool-level error, a JSON-RPC error and a reply this client cannot use are replies. A reply
cannot be used when it does not parse, holds neither a result nor an error, comes with an HTTP
status that is not a success or a redirect this client cannot follow, or is a line past the bound
([MCP-13](#MCP-13)). A server that could not be started or written to, a connection closed before
any reply, and a request not answered within its bound are not replies. A plan run goes on after a
reply and ends at a call that got none ([MANIFEST-15](manifest.md#MANIFEST-15)).

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
`verified-by: bravebot_mcp::http::a_call_that_was_answered_is_told_from_one_that_was_not`
`verified-by: bravebot_mcp::stdio::a_result_of_the_wrong_shape_is_a_reply_and_not_a_transport_failure`

<a id="MCP-6"></a>
### MCP-6: a tool result's text and pictures are taken, and every other part is left out and counted

Several text parts are joined. An `image` part is taken as a picture only where its `mimeType` is
one of `image/png`, `image/jpeg`, `image/gif` and `image/webp`, its `data` is base64 in its
canonical form, and it decodes to between one byte and 8 MiB, the size a picture read from a file
is held to. The picture is held as a `data:` URI this client writes under its own spelling of the
type, so it holds the base64 alphabet and its padding and nothing else the server wrote.

Every other part is left out and counted: an image that fails any of those, audio, an embedded
resource, a link, and a PDF however it arrives. The planner is told how many parts were left out as
a number, and is told nothing of what they were. What a part's `data` and `mimeType` hold is not an
error whatever the type of the value. A part whose `type` or `text` is not a string fails the whole
result. A result that failed carries its text only, and the messages of a prompt are text only
([MCP-11](#MCP-11)).

A picture is held in a slot of its own beside the text's, and takes the label the text takes
([MCP-1](#MCP-1)). It is quarantined whatever that label is, so the planner is given a reference
that names its type and size, which is the byte count of the `data:` URI it is held as. A processor
is given it as a picture, the planner is shown it only after `vet_content`
([VET-4](tools/vet-content.md#VET-4)), and `write_file` writes it as the file it encodes
([WRITE-9](tools/write-file.md#WRITE-9)). The person watching is shown rows saying what it is, how
many bytes it decodes to, the name it is held under and how to look at it, and none of its bytes
([VIEW-3](terminal-transcript.md#VIEW-3)). Which of the four types a server names,
whether it sends a text part, how many pictures it sends and how many parts are left out are the
choices of a server this clause leaves to it, and are recorded as the eighth of the Known costs in
[labels.md](labels.md).

**Why.** A page's screenshot is the result a browser tool exists to return, and it is a tool result
like any other: untrusted, and private because what the server read may be the person's own.
Dropping every part but text meant such a tool could not be offered at all. Taking a picture under
a closed list of types, a size cap and an encoding this client writes itself leaves a server no
string of its own anywhere in what is held, and leaves what a picture is allowed to reach to the
clauses that already say it.

`verified-by: bravebot_mcp::protocol::tool_result_text_is_joined`
`verified-by: bravebot_mcp::protocol::non_text_content_is_ignored`
`verified-by: bravebot_mcp::protocol::a_block_of_another_type_is_ignored_even_when_it_carries_text`
`verified-by: bravebot_mcp::protocol::an_image_part_of_a_listed_type_is_taken_as_a_picture`
`verified-by: bravebot_mcp::protocol::an_image_of_a_type_not_on_the_list_is_left_out_and_counted`
`verified-by: bravebot_mcp::protocol::a_part_that_is_neither_text_nor_a_picture_is_left_out_and_counted`
`verified-by: bravebot_mcp::protocol::data_that_is_not_canonical_base64_is_left_out_and_counted`
`verified-by: bravebot_mcp::protocol::a_picture_past_the_cap_is_left_out_and_one_at_it_is_taken`
`verified-by: bravebot_mcp::protocol::the_size_of_canonical_base64_is_the_size_it_decodes_to`
`verified-by: bravebot_mcp::protocol::what_a_blocks_data_or_media_type_holds_is_never_an_error`
`verified-by: bravebot_mcp::protocol::a_block_whose_type_or_text_is_not_a_string_fails_the_result`
`verified-by: bravebot_agent::mcp::a_picture_a_server_returns_is_never_shown_to_the_planner`
`verified-by: bravebot_agent::mcp::a_part_left_out_of_a_text_only_result_is_told_to_the_planner_as_a_count`
`verified-by: bravebot_agent::mcp::a_servers_picture_is_held_to_the_size_and_types_a_picture_from_a_file_is`
`verified-by: bravebot_agent::mcp::a_picture_a_server_returns_can_be_written_to_a_file_the_person_approves`

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
content is taken, joined in order as a tool result's text is (MCP-6), and a picture in a prompt is
not taken. A result with no text content, one
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

<a id="MCP-12"></a>
### MCP-12: a local server that announces a change to its tools has its list read again and put to a person as a first list is

A stdio server that declared `capabilities.tools.listChanged` as `true` in its handshake may write
`notifications/tools/list_changed`, a message with that method and no id, at any time. Where one is
seen, whether while a request was waiting for its reply or between two requests, the next turn the
person asks for reads `tools/list` again, applies the filters the declaration gave, records in the
trail that the server announced a change, and says so to the person. None of the server's tools is
offered until the list it gives now is put to the person and vouched for, whatever was answered
about the list before it ([SERVERS-8](mcp-servers.md#SERVERS-8)). A list that cannot be read offers
none of the server's tools. A server that did not declare it is not read as having announced a
change, whatever it writes.

The notice's parameters are not read. The one bit taken from it is that the method was named, which
only ever brings forward a question the person would be asked about a new server anyway.

**Why.** What a person vouched for was a list as it stood, and a changed list is a different one. A
tool a server added or redescribed after the vouch must not reach the planner under it. The earlier
answer is not carried over by comparing the two lists, since that comparison would be a decision
taken from the server's bytes ([LABEL-8](labels.md#LABEL-8)).

**A known cost.** A remote server is not read this way. Its requests are one POST each and no
stream is held open between them, so a notice has nowhere to arrive. A prompt a server adds after it
starts is not offered until it starts again, whether or not it announces the change. It is started
holding the declaration's filters as a local one is, so a list read again for it would be held to
them.

`verified-by: bravebot_mcp::stdio::a_tool_list_change_the_server_announced_is_found_once_and_the_list_read_again`
`verified-by: bravebot_agent::mcp::a_tool_list_that_cannot_be_read_again_offers_none_of_the_servers_tools`
`verified-by: bravebot_mcp::stdio::a_notice_from_a_server_that_did_not_declare_it_is_ignored`
`verified-by: bravebot_mcp::protocol::a_server_declares_tool_list_changes_in_its_handshake`
`verified-by: bravebot_mcp::protocol::only_a_message_with_the_method_and_no_id_is_the_notice`
`verified-by: bravebot_agent::mcp::a_tool_list_the_server_announced_a_change_to_is_put_to_the_person_again_before_it_is_offered`
`verified-by: bravebot_agent::mcp::a_tool_list_read_again_after_a_change_is_held_to_the_declarations_filters`
`verified-by: bravebot_agent::servers::a_remote_server_is_started_holding_the_filters_its_declaration_gave`

<a id="MCP-13"></a>
### MCP-13: a line a stdio server writes is read up to a bound, and a longer one is a failure

A reply is one line, and the reader holds a line no longer than 16 MiB, newline not counted. A longer
line is a failure in this client's words, which quote nothing of it, and the reader stops there: the
rest of that line and every line after it are not read. A request made after it fails the same way,
since nothing more is read from that server, and both are told as a reply this client could not use
([MCP-5](#MCP-5)).

**Why.** A server chooses how long its line is, and a reader that holds a whole line holds whatever
it is sent. The bound is above the largest reply a tool can return that this client takes: a picture
at the cap ([MCP-6](#MCP-6)) is a little over 11 MB of base64.

`verified-by: bravebot_mcp::stdio::a_line_up_to_the_limit_is_read_whole`
`verified-by: bravebot_mcp::stdio::a_line_past_the_limit_is_a_failure_and_the_reader_stops_there`
`verified-by: bravebot_mcp::stdio::a_reply_line_past_the_bound_is_a_failure_and_is_not_held`

<a id="MCP-14"></a>
### MCP-14: an argument a person declared as taking a choice is filled from a choice by one gate, after the person answers the call

A server's declaration may name a tool and one of its arguments as taking a choice
([SERVERS-22](mcp-servers.md#SERVERS-22)). A turn is offered `<argument>_ref`, a string naming a
reference, beside that argument, and the argument is not required of it. Where the server lists the
tool with an argument of that name itself, the argument is not offered as a choice for that tool, the
server's own argument is offered as it listed it, and a call that gives it is sent as written
([SERVERS-22](mcp-servers.md#SERVERS-22)). A plan is offered the
tool's own arguments and no `<argument>_ref` ([MANIFEST-15](manifest.md#MANIFEST-15)). A call that
names the reference is accepted only from a context that has met nothing untrusted and only for a
choice nobody has spent ([LABEL-5](labels.md#LABEL-5), [PROC-16](processors.md#PROC-16)). It is
refused, with nobody asked, where the argument is also written out and where more than one choice is
named.

The call is put to the person before the number is carried, whatever a rule, a standing answer or a
mode says ([SERVERS-21](mcp-servers.md#SERVERS-21)). The question draws the argument as the number
and which reference it came from. Where the person answers yes, one gate of the policy layer
consumes the endorsement that answer minted for the tool, refuses arguments that were written in a
context that has met untrusted content or that are private, spends the choice, and hands the number
on as a value that one function of the transport crate can open. That function writes it into the
call as a JSON integer at the declared name and removes `<argument>_ref`. A call whose other
arguments are private is asked about and refused after the answer, and sends nothing. Where the
person answers no, or nobody can be asked, nothing is carried and the choice is not spent. A call
refused for any reason before the gate spends the choice leaves it as it was.

**Why.** The arguments of a call are the planner's own words ([SERVERS-7](mcp-servers.md#SERVERS-7)),
and this process reads none of a processor's answer. The number is a labelled byte in the slot
store, an opaque value in the gate that carries it, and digits on the screen of the person asked,
and nothing compares it. The gates that release it are counted in [labels.md](labels.md), and the
function that writes it into a call is in the transport crate, which is not the driver, so a second
place that fills an argument from a choice is a moved count or a line in a diff.

`verified-by: bravebot_core::policy::carry_a_choice_consumes_the_endorsement_spends_the_choice_and_refuses_before_spending`
`verified-by: bravebot_core::policy::the_trail_of_carrying_a_choice_is_the_same_for_every_number_and_holds_no_digit`
`verified-by: bravebot_core::policy::a_choice_is_drawn_as_digits_to_the_person_and_only_while_it_can_fill_a_call`
`verified-by: bravebot_core::slot::a_choice_is_read_without_being_used_and_used_once`
`verified-by: bravebot_mcp::protocol::a_choice_replaces_the_ref_with_the_integer_and_nothing_else`
`verified-by: bravebot_mcp::protocol::a_choice_is_named_by_its_argument_and_ref`
`verified-by: bravebot_agent::browser_group_servers::a_choice_fills_only_the_argument_the_declaration_named`
`verified-by: bravebot_agent::browser_group_servers::an_undeclared_tool_sends_a_ref_argument_as_written`
`verified-by: bravebot_agent::browser_group_servers::a_server_that_lists_the_second_name_of_a_choice_itself_keeps_that_argument`
`verified-by: bravebot_agent::browser_group_servers::a_choice_call_is_refused_and_sends_nothing_where_the_argument_is_also_written_out`
`verified-by: bravebot_agent::browser_group_servers::a_choice_call_is_refused_and_sends_nothing_where_another_argument_is_private`
`verified-by: bravebot_agent::browser_group_servers::a_choice_call_is_refused_and_sends_nothing_where_the_context_has_met_untrusted_content`
`verified-by: bravebot_agent::browser_group_servers::a_spent_choice_cannot_fill_a_second_call`
`verified-by: bravebot_agent::browser_group_servers::a_refused_call_does_not_spend_the_choice`
`verified-by: bravebot_agent::browser_group_servers::a_no_to_the_question_does_not_spend_the_choice`
`verified-by: bravebot_agent::browser_group_servers::the_question_draws_the_number_and_where_it_came_from`
`verified-by: bravebot_agent::browser_group_servers::no_denial_or_sentence_to_the_planner_holds_a_digit`
`verified-by: bravebot_agent::browser_group_servers::whatever_the_processor_answered_the_driver_makes_the_same_calls`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_with_a_choice`
`verified-by: bravebot_agent::browser_group_servers::a_run_with_nobody_to_ask_refuses_a_call_with_a_choice`
`verified-by: bravebot_agent::browser_group_servers::a_delegate_is_refused_a_call_with_a_choice_with_nobody_asked`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_cannot_name_a_choice`
`verified-by: by-construction (Carried implements no comparison, Hash, Display, conversion to an integer, Deref or AsRef and shows only that a number is there in Debug, so the one thing a caller can do with it is move it into bravebot_mcp::protocol::with_choice; the compile_fail doctests on Carried in crates/core/src/policy.rs pin each of them, and labels.md pins the uses of Carried::into_wire, one of them outside the policy layer, in with_choice)`

<a id="MCP-15"></a>
### MCP-15: a reply the server did not report as failed is described as having reported no failure

The sentence the planner reads beside a quarantined reply to a tool call, and the origin drawn beside
it on the screen and in a plan run, says `what <alias>:<tool> returned with no failure reported` where
the call ended in an answer, and `what <alias>:<tool> said about its failure` where the server marked
the result as an error ([MCP-5](#MCP-5)). It is built from the alias and the tool name the person
read on the list ([SERVERS-7](mcp-servers.md#SERVERS-7)) and from which of the two the transport
settled from the protocol's own error flag. Nothing the server wrote is in it, and the two readings
are the existing split of the call's outcome, so no branch is added on the bytes of a reply.

It says the server reported no failure. It does not say the effect happened, because the flag is the
only fact the driver has about the call. A reply that could not be used, and a call that got no reply,
are described by the driver's own sentences ([MCP-8](#MCP-8)).

**Why.** A planner that makes several writes cannot tell a completed call from a failed one without
reading the reply, and a reply is not read unless a person or a check lets it through.

`verified-by: bravebot_agent::mcp::a_reply_with_no_failure_reported_says_so_and_a_failed_one_does_not`
`verified-by: bravebot_agent::mcp::a_picture_a_server_returns_is_never_shown_to_the_planner`
`verified-by: bravebot_agent::browser_group_servers::what_a_tool_of_input_answers_is_a_reference_and_the_planner_never_holds_it`

<a id="MCP-16"></a>
### MCP-16: a server's tool is offered a boolean `read`, which is the planner's and is never sent to the server

A turn whose tool list holds `vet_content` is offered `read`, a boolean, beside the arguments each
server tool lists, and asks with `read: true` for the reply in the result of the call
([VET-6](tools/vet-content.md#VET-6)). A turn whose list does not hold `vet_content` is not offered
it, as a delegate is not, and a `read` that a call gives there is sent to the server as written.
A plan is offered the tool's own arguments and no `read`
([MANIFEST-15](manifest.md#MANIFEST-15)).

The argument is removed from the call before it reaches the server, so a server never receives a
`read` the planner wrote for the driver. Where the server lists an argument named `read` itself, the
argument is the server's: the tool is offered it as the server listed it, it is not offered as a
request to release the reply, a call that gives it is sent as written, and the reply stays behind its
reference as [MCP-1](#MCP-1) has it. That is the same rule [MCP-14](#MCP-14) applies to a name the
server listed for itself, for the same reason: the name is the server's to define. A `read` that is
neither `true` nor `false` is refused before the call is made, nobody is asked, and nothing is
sent. Absent and null are `false`.

Whether the reply is released in the result, and what the result holds where it is not, are
[VET-6](tools/vet-content.md#VET-6)'s. A reply the server marked as a failure ([MCP-5](#MCP-5)) is
released the same way, and a picture is not ([VET-4](tools/vet-content.md#VET-4)).

**Why.** A name the planner writes for the driver must not be mistaken for an argument of the
server, and an argument the server defined must not be taken from it. The manifest path
(`crates/agent/src/server_calls.rs`) presents no reply to the planner, so it takes no `read`.

`verified-by: bravebot_agent::mcp::a_servers_tool_called_with_read_in_bypass_is_answered_in_the_same_result`
`verified-by: bravebot_agent::mcp::a_servers_tool_called_without_read_in_bypass_still_comes_back_as_a_reference`
`verified-by: bravebot_agent::mcp::a_servers_own_read_argument_is_the_servers_and_asks_for_nothing_to_be_released`
`verified-by: bravebot_agent::mcp::a_servers_tool_called_with_a_read_that_is_not_a_boolean_is_refused_before_the_server_is_called`
`verified-by: bravebot_agent::mcp::a_servers_failed_result_called_with_read_in_bypass_is_answered_in_the_same_result`
`verified-by: bravebot_agent::mcp::a_turn_without_vet_content_is_offered_no_read_on_a_servers_tool`
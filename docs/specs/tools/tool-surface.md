---
id: TOOL
title: The tool surface
status: normative
governs:
  - crates/agent/src/tools.rs
  - crates/agent/src/tools/offered.rs
guards:
  - symbol: Produced::problem
    sites:
      - crates/agent/src/tools.rs: 255
      - crates/agent/src/tools/message.rs: 14
      - crates/agent/src/tools/link.rs: 6
      - crates/agent/src/tools/recall.rs: 25
  - symbol: Produced::refused_with_a_note
    sites:
      - crates/agent/src/tools.rs: 12
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Which tools exist, and which of each call's arguments are routing and which are content. Each tool
has a spec of its own, linked from the table.

## Clauses

<a id="TOOL-1"></a>
### TOOL-1: every argument is routing or content, and the split is fixed here

Routing decides what a tool touches and must be trusted and public. Content is merely carried and
may be untrusted. No argument is both, and nothing at run time reclassifies one.

| Tool | Routing arguments | Content arguments | Result |
|---|---|---|---|
| [`read_file`](read-file.md) | `path`, `path_ref`, `offset`, `limit` | none | the lines, or a reference |
| [`list_files`](list-files.md) | `directory`, `pattern`, `depth` | none | the paths, or a reference per entry |
| [`search`](search.md) | `pattern`, `directory`, `include`, `offset`, `case_sensitive`, `context`, `output` | none | matching lines, files or counts, or a reference |
| [`repo_map`](repo-map.md) | `directory`, `budget` | none | the map text, from files a person vouched for |
| [`read_git`](read-git.md) | `query`, `repository`, `revision`, `path`, `pattern`, `count`, `skip`, `messages`, `since`, `until` | none | the answer, or a reference |
| [`lsp`](lsp.md) | `operation`, `path`, `line`, `character`, `query` | none | locations, with their text shown or referenced |
| [`write_file`](write-file.md) | `path`, `path_ref`, `contents_ref` | `contents` | confirmation |
| [`edit_file`](edit-file.md) | `path`, `path_ref`, `replace_all`, `cell`, `cell_action`, `cell_type` | `old_text`, `new_text`, `edits` | confirmation |
| [`apply_checkout`](../checkouts.md#CHECKOUT-14) | `checkout`, `paths` | none | the files brought back, one confirmation each |
| [`spawn_processor`](spawn-processor.md) | `reads`, `about`, `choose_upto` | `instruction` | a reference |
| [`spawn_agent`](spawn-agent.md) | `kind`, `mcp_servers` | `task`, `each` | one report per delegate |
| [`run`](run.md) | every stage's program and arguments, `directory`, `background`, `deadline_seconds`, `stdin_ref` | standard input | a reference |
| [`read_output`](read-output.md) | the reference naming the result, `offset`, `pattern` | none | the bytes, if a person allows it |
| [`vet_content`](vet-content.md) | the reference naming the slot | none | the bytes, if a person allows it |
| [`job_output`](run.md#RUN-15) | `job`, `kill`, `wait_seconds`, `until_exit` | none | a slice of what it has printed since the previous look, with the count of lines that begin in it, the number of the first, and the references of the earlier looks |
| [`fetch_url`](fetch-url.md) | `url` | none | a reference |
| [`download_url`](download-url.md) | `url`, `path` | none | the path and the byte count |
| [`load_skill`](load-skill.md) | `name` | none | the skill's text |
| [`search_skills`](search-skills.md) | `query` | none | matching skills' names and descriptions, or a count of further matches |
| [`load_tool`](../mcp-servers.md#SERVERS-16) | `name` | none | confirmation that the tool is offered from the next request |
| [`todo_write`](todo-write.md) | none | `todos` | confirmation |
| [`schedule_next`](schedule-next.md) | `delay_seconds`, `noop` | `reason` | the wait that will happen |
| [`watch_file`](watch-file.md) | `path` | none | confirmation that the watch exists |
| [`advisor`](advisor.md) | none | `question` | the advisor's reply |
| [`message_session`](../session-messages.md#MSG-1) | `target` | `text` | confirmation that the message is queued in the other session's process |
| [`ask_user`](ask-user.md) | `questions` | none | what the user answered |
| [`request_path`](request-path.md) | `path`, `write` | none | what the person answered |
| [`propose_plan`](../permission-modes.md#MODE-14) | `plan` | none | what the person answered, and whether plan mode ended |
| [`remember`](remember.md) | `key`, `scope`, `supersedes`, `new` | `text` | confirmation, and the kept facts close to the new one |
| [`propose_rule`](propose-rule.md) | `scope` | `text`, `reason` | what the person answered, and whether the rule was written |
| [`set_link`](set-link.md) | `kind`, `url` | none | confirmation that the address was recorded, or that it was not a word the person typed |
| [`recall`](recall.md) | `query`, `key` | none | the kept facts that match, or the one fact read |

Every tool also takes `why`, which is content on every one of them and is left out of the table for
that reason. [TOOL-5](#TOOL-5) is where that is settled.

Reads return content when it is trusted and a reference when it is not. Writes are silent or shown
according to the trust map.

A flag or a number that shapes a call is routing rather than content: nothing carries it anywhere, so
it is on the same footing as the fields beside it and must be trusted and public. The driver reads
one off the call as the JSON literal it is, since a literal names nothing and holds no text, and
there is nothing in it to promote or to endorse. A routing *string* does name something, and none is
ever read straight off the call: a gate in the policy layer is what hands it over.

Some of those strings name a reference the driver minted instead of a path or a program the planner
composed. Trusted binds them as it binds the rest, but on the context the planner named the
reference in rather than on the string, which arrives as pessimistically wrapped as any other
routing string: a turn whose context has met untrusted content can name no reference at all.
[routing.md](../routing.md) is where that is settled.

`lsp` is the one tool whose result is split across both footings rather than being one or the other:
the line and character of a location are structure, while the name of the file and the text at the
location are content, labelled from the files the answer names and quarantined when any is not
vouched for. [LSP-3](lsp.md#LSP-3) is where that is settled.

`spawn_agent`'s `task` and `each` are the content arguments that may not be untrusted. It decides no
destination, so it is not routing, but it becomes a second planner's prompt rather than a payload
something carries, and a planner's context holds nothing untrusted.
[delegation.md](../delegation.md) is where that is settled.

`verified-by: bravebot_core::policy::routing_refuses_untrusted_values`
`verified-by: bravebot_core::policy::routing_refuses_private_values`
`verified-by: bravebot_core::policy::fetched_content_can_be_written_but_cannot_choose_the_path`
`verified-by: bravebot_agent::tools::a_reference_destination_is_refused_once_the_context_has_met_something_untrusted`

<a id="TOOL-2"></a>
### TOOL-2: before adding a tool, ask what its routing field is

If a person could not approve that field alone, the tool does not get built. A shell string is
destination and payload at once, which is why the planner has no shell and why `apply_patch` is
excluded. An argument vector passes the test, which is why running a pipeline of stages does not.

`verified-by: bravebot_agent::tools::the_tool_set_is_reads_plus_gated_writes`
`verified-by: bravebot_agent::tools::only_run_takes_a_command_line`

<a id="TOOL-3"></a>
### TOOL-3: an unknown tool is reported to the planner rather than ignored

`verified-by: bravebot_agent::turn::an_unknown_tool_is_reported_to_the_model`
`verified-by: bravebot_agent::turn::a_refused_call_is_reported_as_one`
`verified-by: bravebot_agent::turn::each_tool_call_is_announced_before_it_runs_and_summarised_after`

<a id="TOOL-4"></a>
### TOOL-4: a refusal is worded by the driver

The sentence a call fails or is refused with is written here. Its wording is this repository's, and
the values in it are ones the driver may name: a routing argument a gate released, a reference the
driver minted, a name out of a fixed set, and what this machine said about a call this process
made. A sentence somebody on the other side of the call composed is not part of it, whether it
arrived as a server's error message, as a page, or as what a program printed, and however well it
would explain the failure.

A directory or a file named in a refusal is spelled as the planner wrote it. Where a symbolic link in
a tree nobody vouched for led is a name whoever wrote the tree chose, so it is not among the values
above, and a refusal never spells it.

Where their own account of what went wrong is worth keeping, it reaches the person watching through
the release that puts content on a screen, or it is quarantined and named by a reference. The
planner is told the fact of the failure, the call it was about, and what to do differently.

**Why.** A refusal is the one result that is trusted whatever the call touched. A read of a file
nobody vouched for comes back as a reference, and the refusal of that same read comes back as prose
the planner is sent verbatim and uncapped with the driver's attribution on it. So an error type that
carries somebody else's sentence in a plain string, and a call site that formats that error into a
refusal, are together a way into the planner's context that every other road out of a tool closes,
and both ends of it read as ordinary code. Dropping the account altogether would leave a person no
way to find out that their own server is misconfigured, which is why it goes to a screen rather than
nowhere.

The places a refusal is built are pinned in this spec's front matter, so a new one is an edit here
and whoever reviews it is asked what footing its text is on.

`verified-by: bravebot_mcp::lib::a_failing_tools_detail_stays_out_of_the_error_message`
`verified-by: bravebot_lsp::server::a_server_failure_reports_a_code_and_not_the_servers_words`
`verified-by: bravebot_agent::turn::a_failed_fetch_names_the_url_that_was_asked_for_and_not_where_a_redirect_went`
`verified-by: bravebot_agent::turn::a_fetch_refused_for_leaving_its_host_names_no_host_the_server_chose`
`verified-by: bravebot_agent::turn::a_credential_created_as_a_whole_file_is_not_created_and_the_planner_is_told_so`
`verified-by: bravebot_agent::tools::a_failed_listing_names_the_directory_as_typed_and_not_where_it_landed`
`verified-by: bravebot_agent::turn::a_background_line_that_did_not_start_names_the_directory_as_typed`
`verified-by: bravebot_agent::turn::a_credential_refusal_names_the_redirection_under_the_directory_as_typed`
`verified-by: bravebot_agent::turn::a_refused_redirection_is_named_under_the_directory_as_typed`
`verified-by: bravebot_agent::turn::an_unopened_redirection_is_named_under_the_directory_as_typed`

<a id="TOOL-5"></a>
### TOOL-5: every tool asks why it is called, and only a screen reads the answer

Every tool the planner or a delegate is offered takes `why`, one line in the planner's own words
saying what the call is for, and lists it as required. It is asked of each call rather than of each
round, because a round of calls made for different reasons is explained by one line before it only
as far as the reason that line happened to give.

`why` is content. It is labelled as the planner's own output and released to the screen that draws
the call, on the line the call starts with and on the line it finishes with, and it is kept with the
call for a resumed transcript to draw. No tool reads it and nothing decides on it: a call runs the
same with any reason or with none, and whether there is anything to draw is the screen's question to
ask of the text. [VIEW-25](../terminal-transcript.md#VIEW-25) is what each screen draws.

A server's tool is offered as the server describes it and is not asked for a reason, because the
server is sent the arguments as they stand and the person approving the call reads them.

**Why.** A person watching sees every call and what it touches, and without this nothing of what it
was for, so a session reads as a list of commands.

`verified-by: bravebot_agent::tools::every_tool_offered_asks_why_it_is_being_called`
`verified-by: bravebot_agent::turn::each_call_is_announced_and_summarised_with_its_own_reason`
`verified-by: bravebot_agent::tools::a_held_call_is_drawn_with_the_reason_the_planner_gave`
`verified-by: bravebot_agent::tools::a_held_call_to_a_servers_tool_is_drawn_with_its_target`

<a id="TOOL-6"></a>
### TOOL-6: what a tool says names only the tools the same turn is offered

A description and a refusal are read in the turn they were written for, and two turns are not offered
the same list. A delegate that may read files and not run programs is offered `read_git` and no
`run`; a turn a person addressed to a definition is offered what that definition kept.
[delegation.md](../delegation.md) and
[addressing-a-definition.md](../addressing-a-definition.md) are where those two lists are settled. So
a sentence that sends the planner to another tool is written against this turn's own list, and where
that tool is not on it the sentence says what can be done without it instead.

The same holds for what the planner is told about content it may not read. A reference names
`vet_content`, `spawn_processor`, `write_file` and, for output kept whole past a cap, `read_output`
only where the turn's list holds them. So does the answer to a read of a reference to a file the
planner may not read, which names `spawn_processor` and `write_file`. The processor's description
names `write_file`, `vet_content`, `run` and `read_output` only where it does, and the output's names
`vet_content` only where it does. What the processor's description says about text made from
content the planner was not shown names `run` only where the list holds one, and sends the answer
to a program through a file, which names `write_file`, only where the list holds it
([SPAWN-6](spawn-processor.md#SPAWN-6)). `fetch_url` names `spawn_processor`, `write_file` and
`vet_content` only where it does, `job_output` names `read_output`, `spawn_processor` and
`write_file`, and `run` names those three in what it says to do with output that comes back as a
reference, and `job_output` in its `background` argument. The sentence in the output of `run` that
sends text made from it to the processor names `spawn_processor` only where the list holds it.

The texts a tool writes into its result are written the same way. The advice on a run's quarantined
result names `read_output`, `spawn_processor` and `read_file` each only where the list holds it, and
says only what the quarantine is about where it holds none of them ([RUN-14](run.md#RUN-14)). The
sentence on git history names `read_git`, and the account of an ended job names `read_output` and
`read_file`, only where the list holds them. The notes that start a job, move a line to the
background and remind a later turn of a kept job name `job_output` only where the list holds it,
and keep the sentence not to start the job again. The note on a file past the size cap that
`read_git` shows names `read_file` and `run`, and the heading over the files it could not compare
names `run`, each only where the list holds them. The refusal of an empty `old_text` names
`write_file`, the refusal of an editor names `edit_file`, the refusal of `read_output` for a
reference no program printed names `vet_content`, the refusals of `vet_content` and the empty-`reads`
error of `spawn_processor` name `spawn_processor` and `write_file`, the refusal of `lsp` for a server
that cannot be confined names `search` and `read_file`, and the refusal of a processor after private
content names `run`, `read_output` and `vet_content`, each only where the list holds the tool. The
listing of the files kept beside a skill names `read_file` only where the list holds it and gives
the path either way. The sentences about `request_path` in the description of `run` and in the
refusals and notices of confinement are said only where the list holds `request_path`
([PATHREQ-5](request-path.md#PATHREQ-5), [SANDBOX-19](../sandboxing.md#SANDBOX-19),
[SANDBOX-22](../sandboxing.md#SANDBOX-22)).

The descriptions are written from the list as well. `run` names `read_file` and `job_output` in its
paragraph on watching a program, and `read_output` in its `read` argument, each only where the list
holds it. `read_file` names `spawn_processor` for a picture, a PDF and the saved outputs of a
notebook cell, and `edit_file` for a notebook, `write_file` names `spawn_processor` in its
`contents_ref` argument, `edit_file` names `write_file`, `repo_map` and `lsp` name `read_file`, and
`vet_content` names `spawn_processor`, each only where the list holds it.

Two refusals are written from the list rather than from the policy's general denial. That denial
ends "Use a reference to it instead", which neither tool can follow: neither returns a reference at
the point of refusal, and a turn offered no processor cannot use one. The
refusal of `read_file` with `output_of` for a notebook the driver may not examine says the outputs
are closed to `read_file`, prints the label the driver assigned, and names the routes the turn is
offered: a plain read of the file for a reference to give to `spawn_processor`, said only where the
turn is offered a processor and holds no private content ([PROC-4](../processors.md#PROC-4)), and `run`,
said only where the turn is offered it. The refusal of `read_git` for an answer the driver may not
examine says the answer is closed to `read_git`, prints the label, and names `run` only where the
turn is offered it. Both name the person vouching for the path or the repository on every turn, a
delegate's included, and a turn offered none of the other routes is told that only the person can
open it. Neither names `read_output`, which refuses a reference no program printed. Both are fixed
text about the state of the path, with no byte of the file and no name from it. Neither is reached
while the trust map is unchanged: a read of a path nobody vouched for is answered with a reference
before `output_of` is looked at ([READ-1](read-file.md#READ-1)), and `read_git` examines only an
answer labelled trusted. The refusals are built by functions of their own so that a test can give
them a label the call site would not.

The general denial, which about thirty callers inside the driver receive as an error and do not
show, is not written from the list: any other tool that shows it to a planner says "Use a reference
to it instead" whatever the list holds.

The turn settles its list once and writes these from it ([VET-2](vet-content.md#VET-2)), so a
delegate, a definition's tools line and the command line's `--tools` all give the same result. A
run offered every tool is told every one of these names, in each of these texts.

The description of `job_output` says that a call hands back a slice of what the job printed since the
planner's previous look, that the result gives how many lines begin in the slice and the number the
first has, and that the
earlier slices are in the earlier references the result lists. It names no tool in saying so, so the
wording is the same for every list.

**Why.** A turn told to use a tool nobody offered it spends a round calling a name that is not there
and is refused for a reason it cannot act on, and then has a question to report back on unanswered.
It was also told in its own prompt what it cannot do, so it holds both answers at once and the one in
the tool list is the one that is true.

Not every text is written this way yet. Some results and refusals still name `run`, `read_file`,
`edit_file`, `read_output`, `job_output`, `lsp`, `search_skills`, `load_skill`, `download_url`,
`apply_checkout` or `read_git` without reading the list: the decoding note of a page read, the
result of an approved `request_path`, the unconfined way out that a refused write names, some
refusals of `read_output` and `job_output`, the checkout notes of `spawn_agent`, the descriptions of
`load_skill`, `search_skills`, `download_url` and `ask_user`, the notice about `search_skills`, the
planning, person-facing and environment prompts, the reminder of a kept skill after compaction, and
the refusals of a checkout. A text of this kind is to be written from the list when it is found, and
the test for it is the one the sites above have: a list without the tool, and a list with every
tool as the control.

`verified-by: bravebot_agent::tools::read_git_names_run_only_where_the_delegate_holds_one`
`verified-by: bravebot_agent::tools::a_declined_repository_names_run_only_where_the_turn_holds_one`
`verified-by: bravebot_agent::tools::a_query_off_the_list_names_run_only_where_the_turn_holds_one`
`verified-by: bravebot_agent::tools::ask_user_names_run_only_where_the_turn_holds_one`
`verified-by: bravebot_agent::tools::the_text_made_from_content_not_shown_names_run_only_where_the_turn_holds_one`
`verified-by: bravebot_agent::turn::an_addressed_reader_reads_a_read_git_that_names_no_run`
`verified-by: bravebot_agent::turn::an_addressed_turn_without_run_reads_an_ask_user_that_names_no_run`
`verified-by: bravebot_core::reference::a_reference_made_for_a_run_without_spawning_or_writing_names_only_the_tools_it_has`
`verified-by: bravebot_core::policy::a_reference_names_spawn_processor_and_write_file_exactly_when_the_run_is_offered_them`
`verified-by: bravebot_agent::turn::a_reference_an_addressed_turn_is_handed_names_spawn_processor_and_write_file_only_if_offered`
`verified-by: bravebot_agent::tools::a_list_holding_every_tool_gets_the_processor_and_read_output_descriptions_it_always_had`
`verified-by: bravebot_agent::tools::the_processor_description_names_only_the_tools_the_list_holds`
`verified-by: bravebot_agent::tools::the_write_to_a_file_is_described_only_where_the_list_holds_write_file`
`verified-by: bravebot_agent::tools::the_read_output_description_names_vet_content_only_where_the_list_holds_it`
`verified-by: bravebot_agent::turn::the_descriptions_of_the_processor_and_read_output_follow_the_tools_a_turn_is_offered`
`verified-by: bravebot_agent::turn::a_delegate_is_not_told_to_use_a_tool_it_is_not_offered`
`verified-by: bravebot_agent::turn::the_refusal_to_edit_an_untrusted_file_names_only_the_tools_the_turn_is_offered`
`verified-by: bravebot_agent::tools::a_list_holding_every_tool_gets_the_fetch_job_and_run_texts_it_always_had`
`verified-by: bravebot_agent::tools::the_fetch_description_names_only_the_tools_the_list_holds`
`verified-by: bravebot_agent::tools::the_job_output_description_names_only_the_tools_the_list_holds`
`verified-by: bravebot_agent::tools::the_job_output_description_says_a_look_is_a_slice_and_earlier_slices_are_earlier_references`
`verified-by: bravebot_agent::tools::the_run_description_names_only_the_tools_the_list_holds`
`verified-by: bravebot_agent::turn::the_descriptions_of_fetch_job_output_and_run_follow_the_tools_a_turn_is_offered`
`verified-by: bravebot_agent::turn::reading_a_reference_to_a_file_names_spawn_processor_and_write_file_only_if_offered`
`verified-by: bravebot_agent::turn::a_delegate_reading_a_reference_to_a_file_is_told_only_what_it_can_do`
`verified-by: bravebot_agent::turn::a_reference_to_the_whole_of_a_long_output_names_read_output_only_if_offered`
`verified-by: bravebot_core::reference::kept_output_made_for_a_run_without_read_output_names_no_read_output_and_keeps_the_other_moves`
`verified-by: bravebot_core::policy::a_reference_to_kept_output_names_read_output_exactly_when_the_run_is_offered_it`
`verified-by: bravebot_core::policy::the_policy_says_what_a_reference_may_be_given_to`
`verified-by: bravebot_core::policy::a_branch_makes_references_for_the_run_the_policy_was_told_about`
`verified-by: bravebot_core::policy::the_refusal_of_a_processor_in_a_private_context_names_only_the_tools_the_turn_holds`
`verified-by: bravebot_agent::turn::sentences::a_list_holding_every_tool_reads_the_text_it_always_did`
`verified-by: bravebot_agent::turn::sentences::a_list_without_read_output_is_not_pointed_at_it_in_any_arm`
`verified-by: bravebot_agent::turn::sentences::a_list_without_read_output_keeps_what_names_no_missing_tool`
`verified-by: bravebot_agent::turn::sentences::a_list_without_read_output_is_pointed_at_spawn_processor_only_where_it_holds_it`
`verified-by: bravebot_agent::turn::sentences::a_list_without_read_file_is_not_pointed_at_it`
`verified-by: bravebot_agent::turn::sentences::the_history_sentence_is_left_out_where_the_list_lacks_read_git`
`verified-by: bravebot_agent::turn::sentences::a_result_no_command_printed_has_no_advice_on_a_short_list`
`verified-by: bravebot_agent::tools::beside_is_read_off_the_list_the_turn_holds`
`verified-by: bravebot_agent::tools::the_notes_that_start_a_job_name_job_output_only_where_it_is_offered`
`verified-by: bravebot_agent::tools::the_run_description_on_watching_names_only_the_tools_the_list_holds`
`verified-by: bravebot_agent::tools::the_run_read_argument_names_read_output_only_where_the_list_holds_it`
`verified-by: bravebot_agent::tools::the_read_file_description_names_only_the_tools_the_list_holds`
`verified-by: bravebot_agent::tools::the_write_file_reference_argument_names_spawn_processor_only_where_the_list_holds_it`
`verified-by: bravebot_agent::tools::the_edit_file_description_names_write_file_only_where_the_list_holds_it`
`verified-by: bravebot_agent::tools::the_notebook_refusal_names_only_the_routes_the_turn_holds`
`verified-by: bravebot_agent::tools::a_notebook_nobody_vouched_for_is_refused_in_the_helpers_words`
`verified-by: bravebot_agent::tools::the_git_refusal_names_run_only_where_the_turn_holds_it`
`verified-by: bravebot_agent::tools::an_untrusted_git_answer_is_refused_in_the_helpers_words`
`verified-by: bravebot_agent::tools::the_repo_map_lsp_and_vet_content_descriptions_name_only_the_tools_the_list_holds`
`verified-by: bravebot_agent::tools::an_empty_old_text_names_write_file_only_where_the_list_holds_it`
`verified-by: bravebot_agent::tools::the_confinement_statement_names_request_path_only_where_the_list_holds_it`
`verified-by: bravebot_agent::tools::a_refused_read_of_a_non_output_reference_does_not_name_vet_content_off_the_list`
`verified-by: bravebot_agent::tools::the_processor_and_vet_pointers_name_only_the_tools_the_list_holds`
`verified-by: bravebot_agent::tools::the_lsp_refusal_names_search_and_read_file_only_where_the_list_holds_them`
`verified-by: bravebot_agent::tools::the_files_beside_a_skill_name_read_file_only_where_the_list_holds_it`
`verified-by: bravebot_agent::git::the_note_for_a_file_past_the_size_cap_names_only_the_tools_the_turn_holds`
`verified-by: bravebot_agent::git::the_not_compared_heading_names_run_only_where_the_turn_holds_it`
`verified-by: bravebot_agent::replace::the_empty_pattern_refusal_names_write_file_only_where_the_list_holds_it`
`verified-by: bravebot_agent::cmdline::the_refusal_of_an_editor_names_edit_file_only_where_the_list_holds_it`
`verified-by: bravebot_agent::confine::no_sentence_names_request_path_where_the_list_lacks_it`
`verified-by: bravebot_agent::confine::the_profile_line_names_request_path_only_where_the_list_holds_it`
`verified-by: bravebot_agent::turn::a_quarantined_run_result_names_no_tool_the_definition_was_not_offered`
`verified-by: bravebot_agent::turn::what_an_ended_job_printed_names_no_tool_the_definition_was_not_offered`
`verified-by: bravebot_agent::turn::the_notes_of_a_kept_job_name_job_output_only_where_the_turn_is_offered_it`
`verified-by: bravebot_agent::turn::the_note_of_a_moved_line_names_job_output_only_where_the_turn_is_offered_it`
`verified-by: bravebot_agent::turn::the_refusal_of_a_processor_names_only_the_tools_the_turn_was_offered`
`verified-by: bravebot_agent::turn::a_turn_not_offered_request_path_never_names_it`
`verified-by: bravebot_agent::turn::a_processor_given_nothing_names_write_file_only_where_the_turn_was_offered_it`
`verified-by: bravebot_agent::turn::the_files_beside_a_named_skill_name_read_file_only_where_the_turn_was_offered_it`
`verified-by: bravebot_agent::turn::a_mode_change_to_off_names_request_path_only_where_the_turn_was_offered_it`

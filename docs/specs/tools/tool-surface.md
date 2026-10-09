---
id: TOOL
title: The tool surface
status: normative
governs:
  - crates/agent/src/tools.rs
guards:
  - symbol: Produced::problem
    sites:
      - crates/agent/src/tools.rs: 204
  - symbol: Produced::refused_with_a_note
    sites:
      - crates/agent/src/tools.rs: 8
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
| [`read_git`](read-git.md) | `query`, `repository`, `revision`, `path`, `pattern`, `count`, `skip`, `messages`, `since`, `until` | none | the answer, or a reference |
| [`lsp`](lsp.md) | `operation`, `path`, `line`, `character`, `query` | none | locations, with their text shown or referenced |
| [`write_file`](write-file.md) | `path`, `path_ref`, `contents_ref` | `contents` | confirmation |
| [`edit_file`](edit-file.md) | `path`, `path_ref`, `replace_all` | `old_text`, `new_text`, `edits` | confirmation |
| [`apply_checkout`](../checkouts.md#CHECKOUT-14) | `checkout`, `paths` | none | the files brought back, one confirmation each |
| [`spawn_processor`](spawn-processor.md) | `reads`, `about` | `instruction` | a reference |
| [`spawn_agent`](spawn-agent.md) | `kind`, `mcp_servers` | `task`, `each` | one report per delegate |
| [`run`](run.md) | every stage's program and arguments, `directory`, `background`, `deadline_seconds`, `stdin_ref` | standard input | a reference |
| [`read_output`](read-output.md) | the reference naming the result | none | the bytes, if a person allows it |
| [`vet_content`](vet-content.md) | the reference naming the slot | none | the bytes, if a person allows it |
| [`job_output`](run.md#RUN-15) | `job`, `kill`, `wait_seconds` | none | what it has printed since the last look |
| [`fetch_url`](fetch-url.md) | `url` | none | a reference |
| [`load_skill`](load-skill.md) | `name` | none | the skill's text |
| [`todo_write`](todo-write.md) | none | `todos` | confirmation |
| [`schedule_next`](schedule-next.md) | `delay_seconds`, `noop` | `reason` | the wait that will happen |
| [`watch_file`](watch-file.md) | `path` | none | confirmation that the watch exists |
| [`advisor`](advisor.md) | none | `question` | the advisor's reply |
| [`ask_user`](ask-user.md) | `questions` | none | what the user answered |
| [`request_path`](request-path.md) | `path`, `write` | none | what the person answered |

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

<a id="TOOL-6"></a>
### TOOL-6: what a tool says names only the tools the same turn is offered

A description and a refusal are read in the turn they were written for, and two turns are not offered
the same list. A delegate that may read files and not run programs is offered `read_git` and no
`run`; a turn a person addressed to a definition is offered what that definition kept.
[delegation.md](../delegation.md) and
[addressing-a-definition.md](../addressing-a-definition.md) are where those two lists are settled. So
a sentence that sends the planner to another tool is written against this turn's own list, and where
that tool is not on it the sentence says what can be done without it instead.

**Why.** A turn told to use a tool nobody offered it spends a round calling a name that is not there
and is refused for a reason it cannot act on, and then has a question to report back on unanswered.
It was also told in its own prompt what it cannot do, so it holds both answers at once and the one in
the tool list is the one that is true.

Not every description is written this way yet. The read's and the run's advice to arrange a later
look, the processor's, the run's and the job's mention of writing a file, and the output's mention of
checking a reference each name a tool some delegate is not offered.

`verified-by: bravebot_agent::tools::read_git_names_run_only_where_the_delegate_holds_one`
`verified-by: bravebot_agent::tools::a_declined_repository_names_run_only_where_the_turn_holds_one`
`verified-by: bravebot_agent::tools::a_query_off_the_list_names_run_only_where_the_turn_holds_one`
`verified-by: bravebot_agent::tools::ask_user_names_run_only_where_the_turn_holds_one`
`verified-by: bravebot_agent::turn::an_addressed_reader_reads_a_read_git_that_names_no_run`
`verified-by: bravebot_agent::turn::an_addressed_turn_without_run_reads_an_ask_user_that_names_no_run`

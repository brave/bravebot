---
id: TOOL
title: The tool surface
status: normative
governs:
  - crates/agent/src/tools.rs
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
| [`search`](search.md) | `pattern`, `directory`, `include`, `offset`, `case_sensitive` | none | matching lines, or a reference |
| [`lsp`](lsp.md) | `operation`, `path`, `line`, `character`, `query` | none | locations, with their text shown or referenced |
| [`write_file`](write-file.md) | `path`, `path_ref`, `contents_ref` | `contents` | confirmation |
| [`edit_file`](edit-file.md) | `path`, `path_ref`, `replace_all` | `old_text`, `new_text` | confirmation |
| [`spawn_processor`](spawn-processor.md) | `reads`, `about` | `instruction` | a reference |
| [`spawn_agent`](spawn-agent.md) | `kind` | `task`, `each` | one report per delegate |
| [`run`](run.md) | every stage's program and arguments, `directory`, `background`, `deadline_seconds`, `stdin_ref` | standard input | a reference |
| [`read_output`](read-output.md) | the reference naming the result | none | the bytes, if a person allows it |
| [`vet_content`](vet-content.md) | the reference naming the slot | none | the bytes, if a person allows it |
| [`job_output`](run.md#RUN-15) | `job`, `kill`, `wait_seconds` | none | what it has printed since the last look |
| [`fetch_url`](fetch-url.md) | `url` | none | a reference |
| [`load_skill`](load-skill.md) | `name` | none | the skill's text |
| [`todo_write`](todo-write.md) | none | `todos` | confirmation |
| [`schedule_next`](schedule-next.md) | `delay_seconds`, `noop` | `reason` | the wait that will happen |
| [`watch_file`](watch-file.md) | `path` | none | confirmation that the watch exists |
| [`ask_user`](ask-user.md) | `questions` | none | what the user answered |

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
a location is structure and is reported whatever the trust map says, while the text at that location
is content and is quarantined when it is untrusted. [LSP-3](lsp.md#LSP-3) is where that is settled,
and it is the only place in these specs where a path reaches the planner without having been
vouched for.

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

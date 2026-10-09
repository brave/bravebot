---
id: PATHREQ
title: request_path
status: normative
governs:
  - crates/agent/src/tools.rs
guards:
  - symbol: Policy::record_path_reach
documented-by: docs/website/docs/reference/tools.md
---

## Scope

`request_path` asks the person to let the programs a `run` starts reach one path they otherwise
cannot, for the rest of the session. It is offered where `run` is confined and nowhere else. The
rules for what may be reached, and what the sandbox does with a grant, are
[SANDBOX-28](../sandboxing.md#SANDBOX-28); this is the tool's side of it. The result is a sentence
saying what the person answered.

## Clauses

<a id="PATHREQ-1"></a>
### PATHREQ-1: the path is routing and the reason is content

`path` decides what the programs may reach, so it is read through the routing gate and must be
trusted and public. `write` is a flag that shapes the grant and is routing too. `why` is content,
drawn for the person and recorded, and decides nothing ([TOOL-5](tool-surface.md#TOOL-5)). A call
without a path, or without a reason, is answered with an error and asks nobody.

`verified-by: bravebot_agent::tools::request_path_is_offered_with_run_and_takes_a_path_a_flag_and_a_reason`
`verified-by: bravebot_agent::turn::a_path_needs_a_yes_from_the_person_or_the_mode_that_asks_nothing`

<a id="PATHREQ-2"></a>
### PATHREQ-2: only a yes grants, and a run with nobody to ask refuses

The person is shown the path as it resolves, the access, and the reason, and a yes lets programs
read it, or read and write it, for the session. A no, an interrupt and a run with nobody to ask
grant nothing. The mode that answers every permission question grants it. No settings layer,
project file or permission rule grants one in advance.

`verified-by: bravebot_agent::turn::a_yes_to_a_path_lets_a_program_write_it_and_a_read_only_yes_does_not`
`verified-by: bravebot_agent::turn::a_path_needs_a_yes_from_the_person_or_the_mode_that_asks_nothing`
`verified-by: bravebot_tui::confirm::a_path_prompt_shows_the_path_the_access_the_reason_and_what_a_yes_does_not_do`
`verified-by: bravebot_tui::confirm::a_path_longer_than_the_box_takes_no_yes_until_the_end_of_it_has_been_drawn`

<a id="PATHREQ-3"></a>
### PATHREQ-3: a path an `allowWrite` row refuses is refused, and not asked about

`~`, `/`, a drive root, the home directory and any directory above it, `~/.ssh`, `~/.bravebot`, a
credential location and any path holding a wildcard are refused whatever the person would answer.
The result says so without asking.

**Why.** Asking about a path no answer can grant teaches the person to say no to a question the
program should not have put.

`verified-by: bravebot_sandbox::rules::a_request_is_refused_where_an_allow_write_entry_is`
`verified-by: bravebot_agent::turn::a_path_that_is_refused_as_a_row_is_refused_as_a_request_and_not_asked`

<a id="PATHREQ-4"></a>
### PATHREQ-4: a yes is reach and not trust, and it is recorded

A yes marks nothing trusted ([TRUST-9](../trust-map.md#TRUST-9)) and writes no file. The trace
carries one `path_reach` record for it, and `/status` lists it.

`verified-by: bravebot_agent::turn::a_yes_to_a_path_marks_nothing_trusted_and_is_recorded`
`verified-by: bravebot_tui::status::the_report_lists_the_paths_programs_were_let_reach`

<a id="PATHREQ-5"></a>
### PATHREQ-5: nothing is accepted under `off` or in an untrusted workspace

Under the sandbox mode `off` there is no profile to add to, and a workspace that is not trusted
([TRUST-7](../trust-map.md#TRUST-7)) is not one a planner's request reaches the person about. The
result says the request is not accepted, and nobody is asked.

`verified-by: bravebot_agent::turn::a_path_is_not_asked_for_under_off_or_in_an_untrusted_workspace`

<a id="PATHREQ-6"></a>
### PATHREQ-6: a delegate has no such tool

The tool is in the set no delegate is offered, and a call naming it anyway is answered as an
unknown name ([DELEGATE-12](../delegation.md#DELEGATE-12)).

`verified-by: bravebot_agent::tools::a_delegate_is_offered_no_task_list_and_no_way_to_ask`

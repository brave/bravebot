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
### PATHREQ-4: a person's yes is reach and not trust, and it is recorded

A person's yes marks nothing trusted ([TRUST-9](../trust-map.md#TRUST-9)) and writes no file;
where the mode that answers every question grants the request, it opens the directory
and trusts it as [PATHREQ-7](#PATHREQ-7) says. The trace
carries one `path_reach` record for it, and `/status` lists it. The record says who answered: that
the user let programs reach the path, or, where the mode that answers every question did, the name
of that mode and that nobody was asked. A record that credited a person with an answer the mode gave
would be a person's word nobody said.

`verified-by: bravebot_agent::turn::a_yes_to_a_path_marks_nothing_trusted_and_is_recorded`
`verified-by: bravebot_agent::turn::under_bypass_a_granted_path_is_open_to_the_file_tools`
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

<a id="PATHREQ-7"></a>
### PATHREQ-7: a yes does not reach the file tools, except where the mode that answers every question gave it

A person's yes lets a program reach the path. `read_file`, `write_file`, `edit_file`, `list_files`
and `search` stay confined to the working directory and the directories opened with `/add-dir`
([TRUST-10](../trust-map.md#TRUST-10)), and keep refusing a path that only a yes reaches. The refusal
says to open the directory with `/add-dir`, which also marks it trusted ([TRUST-9](../trust-map.md#TRUST-9)).
`request_path` does not do that, since the path is a string the planner chose and a trust decision
over it would be one made on its say-so ([TRUST-8](../trust-map.md#TRUST-8)).

**Bypass.** Where the mode that answers every permission question
([MODE-4](../permission-modes.md#MODE-4)) grants the request, no person is there to run `/add-dir`
and none is asked, so the grant also opens the directory as `/add-dir` does: reachable by the file
tools, trusted, closed by `/add-dir close` and `/clear`, and carried by `--resume`
([TRUST-9](../trust-map.md#TRUST-9)). It is opened only where `/add-dir` would open it. A path
refused unasked ([PATHREQ-3](#PATHREQ-3)) is refused here too, and a directory inside the project,
a file, or one `permissions.readsStayInWorkspace` keeps the tools out of is not opened, with the
grant for programs left standing and the result saying the file tools were not given it. A directory
that holds the project is opened, and the result says that no delegate is given a checkout while it
is open ([CHECKOUT-7](../checkouts.md#CHECKOUT-7)). The result tells the planner which tools now
reach the path, so it does not ask the person to run `/add-dir`.

**Why.** The text a planner chose is the only thing that decides the path, which is the reason a
person's yes does not vouch for it. In bypass the person has already declared that nothing a session
does needs their word, and a session that stops on a question nobody is there to answer is the one
failure the mode exists to avoid: `pr-fix` makes a worktree beside the project, and its edits stop at
the first file tool.

`verified-by: bravebot_agent::turn::a_yes_to_a_path_does_not_let_the_file_tools_touch_it`
`verified-by: bravebot_agent::turn::outside_bypass_a_yes_to_a_path_opens_nothing_for_the_file_tools`
`verified-by: bravebot_agent::turn::under_bypass_a_granted_path_is_open_to_the_file_tools`
`verified-by: bravebot_agent::turn::under_bypass_a_path_that_is_refused_is_still_refused_and_opens_nothing`
`verified-by: bravebot_agent::turn::under_bypass_a_directory_inside_the_project_is_not_opened`
`verified-by: bravebot_agent::turn::under_bypass_a_file_or_a_kept_in_workspace_is_not_opened`
`verified-by: bravebot_agent::turn::under_bypass_a_directory_holding_the_project_says_it_ends_checkouts`
`verified-by: bravebot_agent::workspace::a_directory_opened_through_a_clone_is_open_in_the_workspace_it_came_from`

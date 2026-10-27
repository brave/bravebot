---
id: PATHREQ
title: request_path
status: normative
governs:
  - crates/agent/src/tools.rs
  - crates/agent/src/tools/offered.rs
  - crates/agent/src/workspace/bypass.rs
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
drawn for the person and recorded, and decides nothing ([TOOL-5](tool-surface.md#TOOL-5)): it is
read through the display release alone, so a call with none, or a blank one, is asked about like any
other. A call without a path is answered with an error and asks nobody.

`verified-by: bravebot_agent::tools::request_path_is_offered_with_run_and_takes_a_path_a_flag_and_a_reason`
`verified-by: bravebot_agent::turn::a_path_needs_a_yes_from_the_person_or_the_mode_that_asks_nothing`
`verified-by: bravebot_agent::turn::a_request_for_a_path_is_asked_about_with_no_reason_a_blank_one_or_a_given_one`
`verified-by: bravebot_agent::turn::a_request_for_a_path_that_names_none_or_a_flag_that_is_no_boolean_asks_nobody`

<a id="PATHREQ-2"></a>
### PATHREQ-2: only a yes grants, and a run with nobody to ask refuses

The person is shown the path as it resolves, the access, and the reason, and a yes lets programs
read it, or read and write it, for the session. A no, an interrupt and a run with nobody to ask
grant nothing. The mode that answers every permission question grants it. No settings layer,
project file or permission rule grants one in advance.

`verified-by: bravebot_agent::turn::a_yes_to_a_path_lets_a_program_write_it_and_a_read_only_yes_does_not`
`verified-by: bravebot_agent::turn::a_path_needs_a_yes_from_the_person_or_the_mode_that_asks_nothing`
`verified-by: bravebot_tui::confirm::grants::a_path_prompt_shows_the_path_the_access_the_reason_and_what_a_yes_does_not_do`
`verified-by: bravebot_tui::confirm::grants::a_path_longer_than_the_box_takes_no_yes_until_the_end_of_it_has_been_drawn`

<a id="PATHREQ-3"></a>
### PATHREQ-3: a path an `allowWrite` row refuses is refused, and not asked about

`~`, `/`, a drive root, the home directory and any directory above it, `~/.ssh`, `~/.bravebot`, a
credential location and any path holding a wildcard are refused whatever the person would answer;
a request to read `~/.bravebot/sessions` or a path inside it is the one exception, and a request to
write there is refused like the rest of `~/.bravebot`. The result says so without asking, says what is true of the path that made it refuse (it holds
credentials, a write there could leave code that runs later, it is too broad, or the person's own
deny list covers it) and not which setting decided it, and for a request to write points at the
`run` argument that lets the person approve one line ([SANDBOX-29](../sandboxing.md#SANDBOX-29))
where the session would accept it.

**Why.** Asking about a path no answer can grant teaches the person to say no to a question the
program should not have put.

`verified-by: bravebot_sandbox::rules::a_request_is_refused_where_an_allow_write_entry_is`
`verified-by: bravebot_sandbox::rules::only_a_read_of_the_session_records_is_granted_in_the_state_directory`
`verified-by: bravebot_agent::turn::a_path_that_is_refused_as_a_row_is_refused_as_a_request_and_not_asked`
`verified-by: bravebot_agent::turn::a_refused_path_request_says_why_and_points_a_write_at_the_unconfined_line`

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
### PATHREQ-5: nothing is accepted under `off` or in an untrusted workspace, and no write under `read-only`

Under the sandbox mode `off` there is no profile to add to, and a workspace that is not trusted
([TRUST-7](../trust-map.md#TRUST-7)) is not one a planner's request reaches the person about. Under
`read-only` a request to write is refused and a request to read is accepted
([SANDBOX-22](../sandboxing.md#SANDBOX-22)), so the mode that answers every question does not hand
back the writes the mode withholds. The result says the request is not accepted, and nobody is
asked. A session that accepts no request does not point the planner at the route: the `run`
description and the sentence after a failed step name neither `request_path` nor `run`'s `scopes`
argument there ([SANDBOX-19](../sandboxing.md#SANDBOX-19)). The mode that answers every question
does not give the programs of such a session the rows a request would have added either: a write
beside the session's directories is refused by the operating system, and the sentence, the trail and
`/status` say nothing of a reach through `request_path`
([SANDBOX-28](../sandboxing.md#SANDBOX-28)).

`verified-by: bravebot_agent::turn::a_path_is_not_asked_for_under_off_or_in_an_untrusted_workspace`
`verified-by: bravebot_agent::turn::under_read_only_a_request_to_write_is_refused_and_one_to_read_is_not`
`verified-by: bravebot_agent::turn::in_an_untrusted_workspace_the_description_and_a_failed_run_name_no_menu_and_no_request_path`
`verified-by: bravebot_agent::turn::in_an_untrusted_workspace_a_bypass_run_is_given_no_write_a_request_would_add`

<a id="PATHREQ-6"></a>
### PATHREQ-6: a delegate has no such tool

The tool is in the set no delegate is offered, and a call naming it anyway is answered as an
unknown name ([DELEGATE-12](../delegation.md#DELEGATE-12)).

`verified-by: bravebot_agent::tools::a_delegate_is_offered_no_task_list_and_no_way_to_ask`
`verified-by: bravebot_agent::tools::a_delegate_that_names_request_path_is_told_no_such_tool_and_nobody_is_asked`

<a id="PATHREQ-7"></a>
### PATHREQ-7: a yes does not reach the file tools, and the mode that answers every question reaches them with no request

A person's yes lets a program reach the path. `read_file`, `write_file`, `edit_file`, `list_files`
and `search` stay confined to the working directory and the directories opened with `/add-dir`
([TRUST-10](../trust-map.md#TRUST-10)), and keep refusing a path that only a yes reaches. The refusal
says to open the directory with `/add-dir`, which also marks it trusted ([TRUST-9](../trust-map.md#TRUST-9)).
`request_path` does not do that, since the path is a string the planner chose and a trust decision
over it would be one made on its say-so ([TRUST-8](../trust-map.md#TRUST-8)).

**The file tools in bypass.** While the permission mode is the one that answers every permission
question ([MODE-4](../permission-modes.md#MODE-4)), a path outside the workspace is reachable to a
tool that names one, wherever the account can reach it, with no call to `request_path` and no
directory opened. `read_file`, `write_file`, `edit_file`, `list_files`, `search`, `repo_map`,
`read_git` and `download_url`, `run`'s `directory` and the target of a redirection follow one
rule, which is decided where each resolves a path (`Workspace::resolve` and
`Workspace::confines`), and the tools that name a path are one list a test reads, so a tool added
later is covered or fails it. The refusals are the ones that hold for a granted request, judged
where the path leads through links:

- for a read, the paths [PATHREQ-3](#PATHREQ-3) refuses: a filesystem root, a drive root, the home
  directory and the directories above it, `~/.ssh`, `~/.bravebot` (a read of its `sessions`
  directory excepted), credential locations and the directories that hold one, and a path with a
  control character or a wildcard;
- for a write, the paths [SANDBOX-28](../sandboxing.md#SANDBOX-28) gives a bypass stage no write row
  for: the same, and persistence locations and the directories that hold one. A new entry is judged
  by the directory it lands in, so an entry already inside the home directory or a directory above
  it stays writable and no new entry is made directly in one;
- everything the person's `denyRead` and `denyWrite` cover ([MODE-6](../permission-modes.md#MODE-6)),
  and every path where `permissions.readsStayInWorkspace` is set ([PERM-16](../permissions.md#PERM-16));
- a write under the sandbox mode `read-only`, in every permission mode
  ([SANDBOX-22](../sandboxing.md#SANDBOX-22));
- every path in a workspace the person has not trusted (see PATHREQ-5).

`read_git` reaches a repository outside the workspace and then reads its history only where the
whole of its `.git` is vouched for ([GIT-1](read-git.md#GIT-1)), as it does for a repository in the
workspace nobody vouched for, so its answer there is that refusal. `watch_file` is not given the
reach: a watch is looked at later with no call in progress, so one armed outside the workspace
would end at its first look, and the refusal for a path outside the workspace stands for it.

A refused path gets the refusal it always had and reaches nothing. The sandbox mode does not change
the file tools' reach: it holds under `workspace-write`, `strict` and `off` and on every platform.
The reach is the live mode's and is read at each call, so it ends when the mode changes
([MODE-8](../permission-modes.md#MODE-8)). Nothing is opened or recorded for the directory: no trust
rule is written, the list `/add-dir` keeps is unchanged, and `--resume` and `/add-dir close` have
nothing to carry or close. A file nobody vouched for is put to the vouch question as in the
workspace ([TRUST-8](../trust-map.md#TRUST-8)) and bypass answers it. The trace records the first
reach into each directory as a `path_reach` record, naming the mode, the tool and the argument that
named the path, and `/status` lists the directories. The decision reads the path argument, which is
routing, the live mode, the sandbox mode, the person's rules and facts about the path. It reads no
byte of a file and no program output.

A redirection into a reachable directory needs no `unconfined` argument. `unconfined` is unchanged:
it is asked about every time, since the permission mode does not widen the sandbox mode
([SANDBOX-22](../sandboxing.md#SANDBOX-22), [SANDBOX-29](../sandboxing.md#SANDBOX-29)).

**`request_path` in bypass.** Where the mode that answers every permission question grants the
request, no person is there to run `/add-dir` and none is asked, so the grant also opens the
directory as `/add-dir` does: reachable by the file tools, trusted, closed by `/add-dir close` and
`/clear`, and carried by `--resume` ([TRUST-9](../trust-map.md#TRUST-9)). It is opened only where
`/add-dir` would open it. A path refused unasked ([PATHREQ-3](#PATHREQ-3)) is refused here too, and
a directory inside the project, a file, a directory whose name is not text, or one
`permissions.readsStayInWorkspace` keeps the tools out of is not opened, with the grant for programs
left standing and the result saying the file tools were not given it. A directory that holds the
project is opened, and the result says that no delegate is given a checkout while it is open
([CHECKOUT-7](../checkouts.md#CHECKOUT-7)). The result tells the planner which tools now reach the
path. It remains the way programs get a path under `strict` and on Windows.

The planner cannot see the permission mode and is not told it. The tool's description states the
exception as applying where the person has turned permission prompts off, and says the file tools
reach a path outside the workspace there with no request. A file tool's refusal names `/add-dir`
alone and does not send the planner to `request_path`: where the mode reaches a path the refusal is
not given, and where it does not, a request for the path is refused as well. A skill that depends on
the exception says the same and does not branch on the mode.

The same mode needs no request for programs either, under the sandbox mode `workspace-write` on macOS and
Linux, in a workspace the person has trusted: the stages are given at the start what a request for
writing would be granted ([SANDBOX-28](../sandboxing.md#SANDBOX-28)), so a `run` is not refused
before the planner asks.

**Why.** The text a planner chose is the only thing that decides the path, which is the reason a
person's yes does not vouch for it. In bypass the person has already declared that nothing a session
does needs their word, and a session that stops on a question nobody is there to answer is the one
failure the mode exists to avoid: `pr-fix` makes a worktree outside the project when
`BRAVEBOT_PR_WORKTREES` names a directory there, and its edits stop at the first file tool. Opening
a directory when a file tool first meets it was rejected: it opens one directory per file, writes a
trust rule per directory that outlives bypass and `--resume`, and needs a second mechanism for a
delegate, which has a workspace of its own under `isolation: checkout`.

`verified-by: bravebot_agent::turn::a_yes_to_a_path_does_not_let_the_file_tools_touch_it`
`verified-by: bravebot_agent::turn::outside_bypass_a_yes_to_a_path_opens_nothing_for_the_file_tools`
`verified-by: bravebot_agent::turn::under_bypass_a_granted_path_is_open_to_the_file_tools`
`verified-by: bravebot_agent::turn::under_bypass_the_file_tools_reach_a_sibling_directory_with_no_request`
`verified-by: bravebot_agent::turn::outside_bypass_a_sibling_directory_is_refused_and_no_refusal_names_request_path`
`verified-by: bravebot_agent::turn::leaving_bypass_with_the_key_ends_the_reach_to_a_sibling_directory`
`verified-by: bravebot_agent::turn::under_bypass_a_path_in_each_refusal_category_is_still_refused_and_reaches_nothing`
`verified-by: bravebot_agent::turn::under_bypass_with_read_only_a_sibling_directory_is_read_and_not_written`
`verified-by: bravebot_agent::workspace::a_call_holding_the_reach_reads_and_writes_a_sibling_directory_and_opens_nothing`
`verified-by: bravebot_agent::workspace::a_path_a_request_would_be_refused_is_not_reached_under_bypass`
`verified-by: bravebot_agent::workspace::a_new_entry_directly_in_the_home_directory_or_a_persistence_location_is_refused`
`verified-by: bravebot_agent::workspace::the_persons_deny_lists_and_the_kept_inside_setting_hold_under_bypass`
`verified-by: bravebot_agent::tools::every_tool_that_names_a_path_is_given_the_reach_or_says_why_not`
`verified-by: bravebot_agent::tools::reads_taken_beforehand_hold_the_reach_the_mode_gives_them`
`verified-by: bravebot_tui::status::the_report_lists_the_directories_bypass_took_the_file_tools_into`
`verified-by: bravebot_agent::tools::request_path_says_a_grant_opens_the_file_tools_only_where_prompts_are_off`
`verified-by: bravebot_agent::turn::under_bypass_a_path_that_is_refused_is_still_refused_and_opens_nothing`
`verified-by: bravebot_agent::turn::under_bypass_a_directory_inside_the_project_is_not_opened`
`verified-by: bravebot_agent::turn::under_bypass_a_file_or_a_kept_in_workspace_is_not_opened`
`verified-by: bravebot_agent::turn::under_bypass_a_directory_whose_name_is_not_text_is_not_opened`
`verified-by: bravebot_agent::turn::under_bypass_a_directory_holding_the_project_says_it_ends_checkouts`
`verified-by: bravebot_agent::workspace::a_directory_opened_through_a_clone_is_open_in_the_workspace_it_came_from`

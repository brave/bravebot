---
id: HOST
title: The local host
status: normative
governs:
  - crates/ui-bridge/src/host.rs
  - crates/ui-bridge/src/allowlist.rs
  - crates/ui-bridge/src/workers.rs
  - crates/ui-bridge/src/bin/bravebot-host.rs
documented-by: none (internal: the host is started by a person from a command line and driven by a program they chose; nothing in the product launches it yet)
---

## Scope

`bravebot-host`, one process that owns a bridge and keeps its sessions while the programs that drive it come and go. It serves one client at a time over the anonymous channel of [RPCVIEW-8](session-view.md#RPCVIEW-8). It has no socket path, port or token, and no network or remote client.

Not here: what a client may do to a session it did not open (nothing yet), how a client that returns takes over a session, a snapshot of past events for a returning client, controller handover, pairing, and anything about a phone. A session that outlives its client stops its work and waits for those.

## Clauses

<a id="HOST-1"></a>
### HOST-1: the person starts the host, names the client, and asks for each new one

The host is started with `--sessions-root DIR`, one or more `--workspace DIR`, an optional `--settings FILE`, and `--` followed by the client program and its arguments. The root and the client program are absolute paths. The client is not found on a search path and nothing about it comes from a file a tool could write. A command line that leaves any of these out, or gives a relative root or client, is refused with exit status 2 before a directory is made. A client program or argument that is not valid UTF-8 is passed on as it is. A `--workspace` that is not a directory stops the host with status 1 at startup, also before a directory is made. `--version` prints the version whatever the platform said about inspection. On Linux the host asks the kernel to refuse same-user inspection of its process as the first thing it does ([RPCVIEW-8](session-view.md#RPCVIEW-8)), and exits with status 1 when that fails, because a process that can copy the host's descriptors can hold a channel that is not the client's.

The host starts the client over a new channel when it starts. The client ending ends that channel and leaves the host running. The person starts the next client by typing `launch` on the host's standard input; `quit`, or the end of that input, stops the host, whether or not a client is connected: it ends the connection being served, closes every session and waits up to ten seconds for the threads working for them, so a turn writing its record is not cut off, and says on standard error if any were still working. Nothing a client sends starts a client, and nothing else reads the host's standard input. A line on it that is not text is reported and skipped; a read error, or the end of the input, is reported on standard error before the host stops.

`verified-by: bravebot_ui_bridge::host_process::a_command_line_that_names_no_client_or_a_relative_one_is_refused_before_anything_is_made`
`verified-by: bravebot_ui_bridge::host_process::the_host_stops_when_told_to_or_when_its_input_ends`
`verified-by: bravebot_ui_bridge::host_process::a_workspace_that_does_not_exist_stops_the_host_before_anything_is_made`
`verified-by: bravebot_ui_bridge::closing_tests::stopping_everything_closes_each_session_and_returns_its_running_workers`
`verified-by: bravebot_ui_bridge::host::a_failed_attempt_to_refuse_inspection_is_an_error_and_the_other_outcomes_are_named`
`verified-by: bravebot_ui_bridge::layering::every_binary_that_speaks_over_its_own_stdio_marks_itself_non_dumpable`
`verified-by: bravebot_ui_bridge::host_process::a_client_that_leaves_stops_its_work_and_the_next_client_cannot_act_on_the_session`

<a id="HOST-2"></a>
### HOST-2: a session opens only in a workspace the person gave

`session.new` names an absolute directory, and the host opens it only if it is, after links and `..` are resolved, one of the `--workspace` directories. Any other directory, a link to one, a path that does not exist, `/` and a relative path (which would mean whatever the host's own directory makes it) are refused with `not_allowed` and open nothing. The bridge is given the directory that was checked, not the name that led to it, so a link retargeted after the check cannot take the session elsewhere.

`verified-by: bravebot_ui_bridge::host::a_session_cannot_be_opened_outside_the_workspaces`
`verified-by: bravebot_ui_bridge::host_process::a_session_opens_in_the_directory_that_was_checked`
`verified-by: bravebot_ui_bridge::host_process::a_relative_directory_is_refused_even_from_inside_a_workspace`

<a id="HOST-3"></a>
### HOST-3: a client may ask only what is named

The host answers the methods in `allowlist::OFFERED` with the parameters listed for each, and refuses every other method with `not_allowed`, including a method the bridge answers and one it does not. A parameter that the method's entry does not name is refused too: `turn.send` takes `session` and `prompt`, `run.reply` takes no `remember`, and `trust.reply` takes no `remember`. `trust.reply` is the one widening answer that is offered: the client is the program the person named, over a channel it shares with the host alone, so its answer is the person's own. A client that is not that is not offered it. Refused requests reach no bridge method.

Every method the bridge answers is either offered or listed in `allowlist::DENIED`. A test reads the bridge's dispatch and fails while it answers a method that neither list names, and while either list names one it does not answer, so a method added to the bridge is not reachable through the host until someone classifies it.

The replies that widen what a session may do or read (`output.reply`, `vouch.reply`, `vet.reply`, `server.reply`, `exposure.reply`, `host.reply`, the MCP replies and `manifest.reply`) are denied. A question of one of those kinds cannot be answered through the host. It stays unanswered until it expires or the session is cancelled or closed, or its client leaves, which refuse it.

`verified-by: bravebot_ui_bridge::host::a_method_the_host_does_not_offer_is_refused_and_does_nothing`
`verified-by: bravebot_ui_bridge::allowlist::a_method_the_host_does_not_name_is_refused_whether_or_not_the_bridge_has_it`
`verified-by: bravebot_ui_bridge::allowlist::a_parameter_the_method_does_not_name_is_refused`
`verified-by: bravebot_ui_bridge::allowlist::parameters_that_are_not_an_object_are_refused`
`verified-by: bravebot_ui_bridge::allowlist::no_method_is_both_offered_and_denied`
`verified-by: bravebot_ui_bridge::allowlist::an_offered_method_with_its_own_parameters_is_accepted`
`verified-by: bravebot_ui_bridge::host_allowlist::the_allowlist_classifies_every_method_the_bridge_answers`
`verified-by: bravebot_ui_bridge::host_allowlist::the_scan_reads_arms_with_several_names`

<a id="HOST-4"></a>
### HOST-4: the host's records are in a directory of its own

The host keeps every session record it reads, lists or writes under `--sessions-root`, in place of the `sessions` directory of the state directory. Only that directory moves: settings, trust, credentials and prompt history stay where they are. A terminal or a window running on the same home does not list a record the host wrote and cannot open one by naming its id. The host makes the root readable by its owner only when it makes it. A root that already exists is used only if it is a directory (not a link) that other users cannot reach; the host does not change the mode of a directory it did not make, and refuses to start on one that others can reach or that it cannot write to, which it finds by writing a marker file there at startup. A process in an incognito session, or on a machine with no state directory, still writes nothing.

The root is chosen once, before the first record is read or written, and a second choice is refused and the host says which root is in force (`sessions::keep_in`). The directory the host was started in contributes no settings of its own to the process-wide answers (the network its programs keep, the lists of paths, the hosts a stage is held to): only the person's settings, the file named with `--settings` and the managed pin do.

`verified-by: bravebot_ui_bridge::host_process::the_host_keeps_its_records_apart_from_a_windows`
`verified-by: bravebot_ui_bridge::host_process::an_existing_sessions_root_that_others_can_read_or_a_link_is_refused`
`verified-by: bravebot_session::sessions::a_chosen_root_replaces_the_sessions_directory_and_only_that`
`verified-by: bravebot_session::sessions::a_chosen_root_does_not_make_a_session_writable_where_there_is_no_state_directory`
`verified-by: bravebot_session::keeping_in_a_root::a_second_choice_is_refused_and_names_the_root_in_force`

<a id="HOST-5"></a>
### HOST-5: the host has an instance that changes when it restarts

Each run of the host has an instance ID, drawn from the seed the standard library draws for its hash maps, so a restart gives another. It names a run and is not a credential: nothing is allowed because a client knows it. `agent.info` and `instance.info` report it, and `instance.info` also reports `inspection`, `refused` where the kernel refuses same-user inspection and `not_offered` where the platform has no such switch (macOS), and `agent.info` reports it too. `instance.info` lists the host's sessions with their state (`open`, `closing` or `closed`) and whether a worker is still running for each. A client that recorded an instance learns from a different one that the host restarted and that nothing it recorded about a session is about a session of this run.

`agent.info` and `agent.ready` as the host sends them carry no `home`. Other fields a session reports, such as the path of a trust record it offers to keep, are not filtered.

`verified-by: bravebot_ui_bridge::host::each_host_has_an_instance_of_its_own_and_says_it_without_its_state_directory`
`verified-by: bravebot_ui_bridge::host::the_ready_event_names_the_instance_and_not_the_state_directory`
`verified-by: bravebot_ui_bridge::host::a_second_client_reaches_the_host_the_first_left`


<a id="HOST-6"></a>
### HOST-6: a session is closed when its work has ended

`session.close` stops the session's turn or run, refuses what it waits on, and answers `{"state": "closing"}` at once. The host sends `session.closed` (with `saved: "unknown"`) once every thread that worked for the session has returned and dropped what it held, and not before. Until then the session is `closing`, still counts as working, and keeps its owner, because a turn can still write its record. A thread that panics has ended.

`closed` says that the work has ended. It does not say that the last save succeeded, and no save outcome is reported. A closed or closing session is not found by any later request.

`verified-by: bravebot_ui_bridge::host_process::a_session_is_reported_closed_only_after_its_turn_has_ended`
`verified-by: bravebot_ui_bridge::workers::a_session_is_not_idle_while_a_worker_runs`
`verified-by: bravebot_ui_bridge::workers::a_worker_that_panics_counts_as_ended`
`verified-by: bravebot_ui_bridge::workers::a_worker_counts_as_ended_only_after_what_it_held_is_dropped`
`verified-by: bravebot_ui_bridge::workers::idle_means_every_worker_has_ended`
`verified-by: bravebot_ui_bridge::closing_tests::a_closed_session_is_not_closed_while_its_worker_runs`
`verified-by: bravebot_ui_bridge::closing_tests::a_session_with_no_worker_is_closed_at_once_and_an_unknown_one_is_refused`
`verified-by: bravebot_ui_bridge::closing_tests::a_worker_of_another_session_does_not_keep_a_session_from_closing`
`verified-by: bravebot_ui_bridge::closing_tests::a_session_is_working_exactly_while_a_worker_runs`

<a id="HOST-7"></a>
### HOST-7: a session belongs to the connection that opened it

Requests naming a session are accepted only from the connection that opened it, and events of a session go only to that connection. A later connection sees the session in `instance.info` and is refused every request that names it with `not_allowed`, with one exception: once the connection that opened a session has ended, any later connection may close it with `session.close`, and then owns it for the `session.closed` that follows.

When a connection ends, the turn or run each of its sessions has going is cancelled and the question it waits on is refused, so nothing is left asking a person who is no longer there. The sessions stay open, with no client, until the host stops.

`verified-by: bravebot_ui_bridge::host_process::a_client_that_leaves_stops_its_work_and_the_next_client_cannot_act_on_the_session`
`verified-by: bravebot_ui_bridge::closing_tests::stopping_the_work_of_a_session_with_none_changes_nothing`

<a id="HOST-8"></a>
### HOST-8: a request line has a limit

A write to a client that waits more than five seconds, or fails, ends the connection: requests already read are not executed after that, because nobody can hear their answers. A client that reads nothing therefore cannot stop the host reading the connection and seeing it end. The answer to a `session.close` is written before the `session.closed` that follows it.

A request line is at most 8 MiB, newline excluded. A line that reaches the limit without ending ends the connection without being answered or dispatched. A line of exactly the limit is read. Bytes that arrive without their end of line before the client closes its end are not a request and are not run.

`verified-by: bravebot_ui_bridge::host::a_line_that_does_not_end_ends_the_connection`
`verified-by: bravebot_ui_bridge::host::a_request_at_the_limit_is_read`
`verified-by: bravebot_ui_bridge::host::a_last_line_with_no_newline_is_not_run`
`verified-by: bravebot_ui_bridge::host::a_client_that_never_reads_does_not_wedge_the_host`
`verified-by: bravebot_ui_bridge::host_process::quit_stops_the_host_while_a_client_is_connected`
`verified-by: bravebot_ui_bridge::host_process::a_line_of_input_that_is_not_text_does_not_stop_the_host`
`verified-by: bravebot_ui_bridge::host_process::a_client_path_that_is_not_utf8_is_not_a_panic`
`verified-by: bravebot_ui_bridge::host_process::a_sessions_root_the_host_cannot_write_to_is_refused`
`verified-by: bravebot_ui_bridge::host_process::the_answer_to_a_close_comes_before_the_session_is_said_to_be_closed`

<a id="HOST-9"></a>
### HOST-9: a program the host starts holds no part of the channel

The host holds the near end of its client's channel while it runs the person's hooks and the programs a turn approves. None of them inherits a descriptor of the channel: the ends are close-on-exec, the channel is created and every program is started under the one gate of [RPCVIEW-8](session-view.md#RPCVIEW-8), and the client's end is closed in the host before it serves. An unconfined hook that lists its own descriptors finds no socket. This is shown on the platform the tests run on; Linux has not been run, and a descriptor made in two steps outside the gate is not covered, as the clause linked above says.

`verified-by: bravebot_ui_bridge::host_process::an_unconfined_hook_the_host_runs_holds_no_socket`
`verified-by: bravebot_ui_bridge::host_process::a_program_handed_a_socket_lists_it`
`verified-by: bravebot_ui_bridge::host_process::a_listing_of_either_system_shows_its_sockets`
`verified-by: bravebot_ui_bridge::control_boundary::programs_a_host_starts_hold_no_end_of_the_channels_it_created`

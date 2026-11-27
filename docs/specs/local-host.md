---
id: HOST
title: The local host
status: normative
governs:
  - crates/ui-bridge/src/host.rs
  - crates/ui-bridge/src/host_trust.rs
  - crates/ui-bridge/src/allowlist.rs
  - crates/ui-bridge/src/answerers.rs
  - crates/ui-bridge/src/workers.rs
  - crates/ui-bridge/src/widening.rs
  - crates/ui-bridge/src/device_view.rs
  - crates/ui-bridge/src/bin/bravebot-host.rs
  - packages/agent-client/scripts/dev-relay.ts
  - packages/agent-client/scripts/host-client.ts
  - packages/agent-client/scripts/websocket.ts
  - packages/agent-client/src/dev-relay/index.ts
documented-by: none (internal: the host is started by a person from a command line and driven by a program they chose; nothing in the product launches it yet)
---

## Scope

`bravebot-host`, one process that owns a bridge and keeps its sessions while the programs that drive it come and go. It serves every client that is connected, each over an anonymous channel of its own ([RPCVIEW-8](session-view.md#RPCVIEW-8)), and reads each connection on a thread of its own. Started without `--listen`, the host itself has no socket path, port or token, and no network or remote client. Started with it, the host holds one TCP port at the address it was given and no other socket another program can find, and serves only a device the person paired ([LISTEN](device-listener.md)). The development relay of [HOST-12](#HOST-12) is a client program that the host starts like any other. It listens on a loopback port so that an app in development can reach the host.

Not here: more of a transcript for a client that returns than the most recent rows of [RPCVIEW-9](session-view.md#RPCVIEW-9), how a device is paired and what it is answered, which is [LISTEN](device-listener.md), and anything about a phone beyond the development relay and that listener.

## Clauses

<a id="HOST-1"></a>
### HOST-1: the person starts the host, names the clients, and asks for each new one

The host is started with `--sessions-root DIR`, one or more `--workspace DIR`, an optional `--settings FILE`, an optional `--listen ADDR:PORT`, and one or more clients. Each client is `--client NAME --answers KINDS --program PATH`, followed by any number of `--arg VALUE`. NAME is 1 to 32 of `a-z`, `0-9` and `-`, and no two clients have the same one. KINDS is the answers the client may give, a comma-separated list of `confirm`, `run`, `fetch`, `ask` and `trust` with each named once, or `none`; there is no default. PATH is the client program. `--arg` takes the next argument as it is, so a value such as `--` or one that is not valid UTF-8 is passed on unchanged. The root and each program are absolute paths. A program is not found on a search path, and nothing about a client comes from a file a tool could write or from anything a client sends. `--listen` names one address that devices reach the host at, as an IP address and a port (a port of 0 lets the system pick one, which the host then says). It does not name every interface: `0.0.0.0` and `::` are refused, and so is an address that is not ADDR:PORT and a second `--listen`. An address that cannot be bound stops the host with status 1 after the startup question about each workspace and before any client starts, and says why. The host says on standard error the address it listens on and the hash of its key, in groups so a person can read it. A command line that leaves any of these out, names a client twice, names an unknown answer, gives a relative root or program, or has a `--` that is not the value of an `--arg` is refused with exit status 2 before a directory is made. A client program that is not valid UTF-8 is passed on as it is. A `--workspace` that is not a directory stops the host with status 1 at startup, also before a directory is made. `--version` prints the version whatever the platform said about inspection. On Linux the host asks the kernel to refuse same-user inspection of its process as the first thing it does ([RPCVIEW-8](session-view.md#RPCVIEW-8)), and exits with status 1 when that fails, because a process that can copy the host's descriptors can hold a channel that is not the client's.

Before it starts any client, the host asks on its standard error about each `--workspace`, in the order given, whether to trust it ([TRUST-23](trust-map.md#TRUST-23)). It puts the same lines and the same question as a session in lines ([CLI-14](cli.md#CLI-14)), and reads the answer from its standard input before it reads any `launch` or `quit`. A yes is kept in the record the terminal and the desktop interface read, in a line that names no session, so a session later opened in that workspace starts trusted and its client is not asked. A no, or any line that is not yes, keeps nothing, and each session there asks its client as before. `quit` at the question stops the host with nothing kept about that workspace and no client started. The end of the input there, or a read of it that fails, answers no and stops the host in the same way. A workspace that a kept yes already answers for is not asked about, and the host says so, when the yes was given and how to be asked again. Nor is one where a yes cannot be kept: the home directory or one holding it, a filesystem root, a directory whose creation time the filesystem does not give, or a process that may write nothing. A yes there would answer nothing, so the host says that each session there asks. The person typed the command line and types the answer, so the answer is theirs.

When it starts, the host starts every client at once, in the order of the command line, each over a new channel of its own, and serves them all at once. A client ending ends its channel and leaves the host running. The person starts a client again by typing `launch NAME` on the host's standard input. A bare `launch` starts the only client where there is one, and is refused where there are several. A `launch` of a name no client has, or of a client that is connected, is refused on standard error and not kept for later. A client counts as connected from its launch until the host has finished dealing with the loss of its connection. The host then says on standard error that the client ended, and a `launch` of it typed from then on is accepted. `quit`, or the end of that input, stops the host, whether or not a client is connected: it ends every connection, closes every session and waits up to ten seconds for the threads working for them, so a turn writing its record is not cut off, and says on standard error if any were still working. Where the host listens it also takes `pair NAME read|control [WORKSPACE]`, `confirm CODE`, `devices` and `revoke NAME` on that input ([LISTEN-5](device-listener.md#LISTEN-5)), and where it does not listen it answers those words by saying to start it with `--listen`. Nothing a client or a device sends starts a client or is read as one of these commands, and nothing but the startup question and these commands reads the host's standard input. A line on it that is not text is reported and skipped; a read error, or the end of the input, is reported on standard error before the host stops.

`verified-by: bravebot_ui_bridge::host_process::a_command_line_that_names_no_client_or_a_relative_one_is_refused_before_anything_is_made`
`verified-by: bravebot_ui_bridge::bin::bravebot-host::each_client_keeps_what_its_group_named_in_order`
`verified-by: bravebot_ui_bridge::bin::bravebot-host::a_client_that_is_not_fully_named_is_refused`
`verified-by: bravebot_ui_bridge::allowlist::a_list_of_answers_names_known_words_once_or_none`
`verified-by: bravebot_ui_bridge::host_process::every_client_starts_once_at_once_and_a_launch_starts_one_that_ended`
`verified-by: bravebot_ui_bridge::host_process::a_launch_of_an_unknown_or_connected_client_is_refused_and_not_kept`
`verified-by: bravebot_ui_bridge::host_process::quit_with_two_clients_connected_ends_both_and_closes_every_session`
`verified-by: bravebot_ui_bridge::host_process::the_host_stops_when_told_to_or_when_its_input_ends`
`verified-by: bravebot_ui_bridge::device_listener::a_listen_that_does_not_name_one_address_is_refused_before_anything_is_made`
`verified-by: bravebot_ui_bridge::device_listener::a_listen_that_cannot_be_bound_stops_the_host_before_its_client_starts`
`verified-by: bravebot_ui_bridge::device_listener::a_host_not_told_to_listen_says_how_to_pair`
`verified-by: bravebot_ui_bridge::device_listener::quit_ends_a_phones_connection_and_stops_the_host`
`verified-by: bravebot_ui_bridge::host_process::a_workspace_that_does_not_exist_stops_the_host_before_anything_is_made`
`verified-by: bravebot_ui_bridge::host_process::a_workspace_already_trusted_is_not_asked_about`
`verified-by: bravebot_ui_bridge::host_process::a_yes_at_startup_is_kept_and_a_session_there_is_not_asked`
`verified-by: bravebot_ui_bridge::host_process::a_no_at_startup_keeps_nothing_and_a_session_there_asks`
`verified-by: bravebot_ui_bridge::host_process::a_workspace_where_a_yes_cannot_be_kept_is_not_asked_about`
`verified-by: bravebot_ui_bridge::host_process::a_line_that_is_not_yes_declines_and_quit_stops_the_host`
`verified-by: bravebot_ui_bridge::host_process::the_input_ending_at_the_startup_question_stops_the_host_with_nothing_kept`
`verified-by: bravebot_ui_bridge::host_trust::a_line_that_is_not_text_is_a_no_and_the_clients_may_start`
`verified-by: bravebot_ui_bridge::host_trust::input_that_cannot_be_read_stops_the_host_and_keeps_nothing`
`verified-by: bravebot_ui_bridge::host_process::the_clients_start_only_after_the_last_workspace_is_answered`
`verified-by: bravebot_ui_bridge::closing_tests::stopping_everything_closes_each_session_and_returns_its_running_workers`
`verified-by: bravebot_ui_bridge::host::a_failed_attempt_to_refuse_inspection_is_an_error_and_the_other_outcomes_are_named`
`verified-by: bravebot_ui_bridge::layering::every_binary_that_speaks_over_its_own_stdio_marks_itself_non_dumpable`
`verified-by: bravebot_ui_bridge::host_process::a_client_not_attached_to_a_session_cannot_act_on_it_and_may_close_it_once_it_is_left`

<a id="HOST-2"></a>
### HOST-2: a session opens only in a workspace the person gave

`session.new` names an absolute directory, and the host opens it only if it is, after links and `..` are resolved, one of the `--workspace` directories. Any other directory, a link to one, a path that does not exist, `/` and a relative path (which would mean whatever the host's own directory makes it) are refused with `not_allowed` and open nothing. The bridge is given the directory that was checked, not the name that led to it, so a link retargeted after the check cannot take the session elsewhere. A session the host opens asks, and its `session.new` reply says `ask` as `permissionMode`, even where the person's `permissions.startInAuto` opens a desktop window's sessions in auto ([AUTO-1](auto-mode.md#AUTO-1)): the host offers its client no `session.mode`, so a session in auto could not be moved out of it, and the bridge the host runs opens every session asking. A workspace whose name is not text, which the bridge cannot be given, is refused and not opened under a lookalike name. That test needs a file system that allows such a name, so it skips on macOS and runs on Linux.

`verified-by: bravebot_ui_bridge::host::a_session_cannot_be_opened_outside_the_workspaces`
`verified-by: bravebot_ui_bridge::host_process::a_session_opens_in_the_directory_that_was_checked`
`verified-by: bravebot_ui_bridge::host_process::a_hosts_session_asks_where_the_setting_opens_a_windows_in_auto`
`verified-by: bravebot_ui_bridge::dispatch::a_bridge_opening_in_ask_opens_every_session_asking_where_the_setting_says_auto`
`verified-by: bravebot_ui_bridge::host_process::a_relative_directory_is_refused_even_from_inside_a_workspace`
`verified-by: bravebot_ui_bridge::host_process::a_workspace_whose_name_is_not_text_is_refused_not_opened_under_a_lookalike`

<a id="HOST-3"></a>
### HOST-3: a client may ask only what is named

The host answers the methods in `allowlist::OFFERED` with the parameters listed for each, and refuses every other method with `not_allowed`, including a method the bridge answers and one it does not. A parameter that the method's entry does not name is refused too: `turn.send` takes `session`, `prompt` and `messageId`, `turn.lookup` takes `session` and `messageId`, `run.reply` takes no `remember`, and `trust.reply` takes no `remember`. `trust.reply` is the one widening answer that is offered, and only to a client given `trust` ([HOST-1](#HOST-1)): such a client is a program the person named and chose to let answer it, over a channel it shares with the host alone, so its answer is the person's own. The same holds for each reply: `confirm.reply`, `run.reply`, `fetch.reply`, `ask.reply` and `trust.reply` from a client not given `confirm`, `run`, `fetch`, `ask` or `trust` are refused with `not_allowed`. A client reached any other way is not offered any of them. Refused requests reach no bridge method.

Every method the bridge answers is either offered or listed in `allowlist::DENIED`. A test reads the bridge's dispatch and fails while it answers a method that neither list names, and while either list names one it does not answer, so a method added to the bridge is not reachable through the host until someone classifies it.

`session.limit` is the desktop window's `/limit` ([TURN-8](turns.md#TURN-8)). It takes `session` and, optionally, `limit`. Given `limit`, it sets the session's limit: a string read as the `limit` setting is read, or `null` for no limit. Given only `session`, it reads the limit and changes nothing. Either way the result is `{limit, spent}`, where `limit` is `{unit, count}` with `unit` `tokens` or `credits`, or `null` when the session has no limit, and `spent` is what the session has spent in that unit, or in tokens when there is no limit. A limit that is not a string or `null`, or a string that is not a limit, is refused with `bad_request` and changes nothing. A handle that names no open session is refused with `no_such_session`. A limit set while a turn runs applies to that turn's next request. The host denies the method with `session.mode`, because a person at the host decides how its sessions behave, so a host client can neither clear the limit nor raise it. A host session still carries the limit from the person's settings, and its question is the `ask` kind ([HOST-10](#HOST-10)): a session with no attached client given `ask` has the question refused, which ends the turn as a stop.

The replies that widen what a session may do or read (`output.reply`, `vouch.reply`, `vet.reply`, `server.reply`, `exposure.reply`, `host.reply`, the MCP replies and `manifest.reply`) are denied. A question of one of those kinds cannot be answered through the host, so it is refused when it is asked and is not shown ([HOST-10](#HOST-10)).

`verified-by: bravebot_ui_bridge::host::a_method_the_host_does_not_offer_is_refused_and_does_nothing`
`verified-by: bravebot_ui_bridge::allowlist::a_method_the_host_does_not_name_is_refused_whether_or_not_the_bridge_has_it`
`verified-by: bravebot_ui_bridge::allowlist::a_parameter_the_method_does_not_name_is_refused`
`verified-by: bravebot_ui_bridge::allowlist::parameters_that_are_not_an_object_are_refused`
`verified-by: bravebot_ui_bridge::allowlist::no_method_is_both_offered_and_denied`
`verified-by: bravebot_ui_bridge::allowlist::an_offered_method_with_its_own_parameters_is_accepted`
`verified-by: bravebot_ui_bridge::allowlist::a_reply_the_client_may_not_give_is_refused_and_any_other_request_passes`
`verified-by: bravebot_ui_bridge::host::a_reply_the_client_may_not_give_is_refused_before_the_bridge_reads_it`
`verified-by: bravebot_ui_bridge::host_process::a_reply_of_the_wrong_kind_is_refused_and_the_question_stays_answerable`
`verified-by: bravebot_ui_bridge::host_allowlist::the_allowlist_classifies_every_method_the_bridge_answers`
`verified-by: bravebot_ui_bridge::host_allowlist::the_scan_reads_arms_with_several_names`
`verified-by: bravebot_ui_bridge::spend_limit::session_limit_sets_clears_and_reads`
`verified-by: bravebot_ui_bridge::spend_limit::a_limit_set_while_a_turn_runs_holds_its_next_request`

<a id="HOST-4"></a>
### HOST-4: the host's records are in a directory of its own

The host keeps every session record it reads, lists or writes under `--sessions-root`, in place of the `sessions` directory of the state directory. Only that directory moves: settings, trust, credentials and prompt history stay where they are. A terminal or a window running on the same home does not list a record the host wrote and cannot open one by naming its id. The host makes the root readable by its owner only when it makes it. A root that already exists is used only if it is a directory (not a link) that other users cannot reach; the host does not change the mode of a directory it did not make, and refuses to start on one that others can reach or that it cannot write to, which it finds by writing a marker file there at startup. A process in an incognito session, or on a machine with no state directory, still writes nothing.

The root is chosen once, before the first record is read or written, and a second choice is refused and the host says which root is in force (`sessions::keep_in`). The directory the host was started in contributes no settings of its own to the process-wide answers (the network its programs keep, the lists of paths, the hosts a stage is held to): only the person's settings, the file named with `--settings` and the managed pin do.

`verified-by: bravebot_ui_bridge::host_process::the_host_keeps_its_records_apart_from_a_windows`
`verified-by: bravebot_cli::running::the_terminal_does_not_list_or_continue_a_record_in_a_hosts_sessions_root`
`verified-by: bravebot_ui_bridge::host_process::an_existing_sessions_root_that_others_can_read_or_a_link_is_refused`
`verified-by: bravebot_ui_bridge::host_process::a_closed_network_in_the_settings_the_host_was_started_on_reaches_its_turns`
`verified-by: bravebot_session::sessions::a_chosen_root_replaces_the_sessions_directory_and_only_that`
`verified-by: bravebot_session::sessions::a_chosen_root_does_not_make_a_session_writable_where_there_is_no_state_directory`
`verified-by: bravebot_session::keeping_in_a_root::a_second_choice_is_refused_and_names_the_root_in_force`

<a id="HOST-5"></a>
### HOST-5: the host has an instance that changes when it restarts

Each run of the host has an instance ID, drawn from the seed the standard library draws for its hash maps, so a restart gives another. It names a run and is not a credential: nothing is allowed because a client knows it. `agent.info` and `instance.info` report it, and `instance.info` also reports `inspection`, `refused` where the kernel refuses same-user inspection and `not_offered` where the platform has no such switch (macOS), and `agent.info` reports it too. `instance.info` also reports `answers`, the answers the asking client was given on the command line, in the order `confirm`, `run`, `fetch`, `ask`, `trust`; it grants nothing. `instance.info` lists the host's sessions with their state (`open`, `closing` or `closed`), whether a worker is still running for each, and `attached`, whether the asking connection is attached to it ([HOST-7](#HOST-7)). A client that recorded an instance learns from a different one that the host restarted and that nothing it recorded about a session is about a session of this run.

`agent.info` and `agent.ready` as the host sends them carry no `home`. Other fields a session reports, such as the path of a trust record it offers to keep, are not filtered.

`verified-by: bravebot_ui_bridge::host::each_host_has_an_instance_of_its_own_and_says_it_without_its_state_directory`
`verified-by: bravebot_ui_bridge::host::the_ready_event_names_the_instance_and_not_the_state_directory`
`verified-by: bravebot_ui_bridge::host::a_second_client_reaches_the_host_the_first_left`
`verified-by: bravebot_ui_bridge::host::instance_info_names_the_answers_of_the_client_that_asks`
`verified-by: bravebot_ui_bridge::host::clients_connected_at_once_are_each_served_and_each_told_once_that_the_host_is_ready`


<a id="HOST-6"></a>
### HOST-6: a session is closed when its work has ended

`session.close` stops the session's turn or run, refuses what it waits on, and answers `{"state": "closing"}` at once. The host sends `session.closed` (with `saved: "unknown"`) once every thread that worked for the session has returned and dropped what it held, and not before. Until then the session is `closing`, still counts as working, and keeps its owner, because a turn can still write its record. A thread that panics has ended. The hook that ends a session that began ([HOOK-10](hooks.md#HOOK-10)) is one of those threads, and runs after the session's turn has ended, so `session.closed` follows it.

`closed` says that the work has ended. It does not say that the last save succeeded, and no save outcome is reported. A closed or closing session is not found by any later request.

`verified-by: bravebot_ui_bridge::host_process::a_session_is_reported_closed_only_after_its_turn_has_ended`
`verified-by: bravebot_ui_bridge::workers::a_session_is_not_idle_while_a_worker_runs`
`verified-by: bravebot_ui_bridge::workers::a_worker_that_panics_counts_as_ended`
`verified-by: bravebot_ui_bridge::host_process::a_hosted_session_is_reported_closed_only_after_its_end_hook_has_run`
`verified-by: bravebot_ui_bridge::workers::a_worker_counts_as_ended_only_after_what_it_held_is_dropped`
`verified-by: bravebot_ui_bridge::workers::idle_means_every_worker_has_ended`
`verified-by: bravebot_ui_bridge::closing_tests::a_closed_session_is_not_closed_while_its_worker_runs`
`verified-by: bravebot_ui_bridge::closing_tests::a_session_with_no_worker_is_closed_at_once_and_an_unknown_one_is_refused`
`verified-by: bravebot_ui_bridge::closing_tests::a_worker_of_another_session_does_not_keep_a_session_from_closing`
`verified-by: bravebot_ui_bridge::closing_tests::a_session_is_working_exactly_while_a_worker_runs`

<a id="HOST-7"></a>
### HOST-7: a session has a set of attached connections

A session has a set of attached connections. The connection that opens it is attached from `session.new`, and another joins with `session.attach` ([HOST-11](#HOST-11)). There is no controller: each attached connection may send, cancel, reply and close, and a connection that is attached already is refused a second `session.attach` with `bad_request`. There is no request to leave a session other than ending the connection. A connection that is not attached sees the session in `instance.info` with `attached` false, and is refused every other request that names it with `not_allowed`. It may close a session only when no connection is attached.

A session's events go to every attached connection, except one stopped for the session ([HOST-15](#HOST-15)), in the order the bridge emits them, and one held to a ceiling is sent a device's copy of them ([HOST-16](#HOST-16)), because the view and its sequence belong to the session and not to a connection. Two are sent only to the connection whose request made them, and only while it is attached: `session.view.snapshot`, which answers its `session.attach` or `session.view.resync`, and the startup question `trust.request` ([HOST-10](#HOST-10) says which connections are sent that). An event made while a session is being opened goes to the connection opening it. A `session.close` attaches the connection that sent it, so `session.closed` goes to it and to each connection attached when the close was answered, after the closer's answer ([HOST-8](#HOST-8)). The host tells the bridge the name of the client that sent each send, answer, trust answer and cancel it passes on, so the view and the trail can name the client that sent, answered or cancelled ([RPCVIEW-14](session-view.md#RPCVIEW-14), [TRACE-7](trace.md#TRACE-7)).

The first answer to a question counts, and a later answer to it is refused with `no_such_request` ([RPCVIEW-6](session-view.md#RPCVIEW-6)). A `turn.send` while a turn runs is refused with `turn_in_flight`, whichever attached connection sent the turn that is running.

When an attached connection ends it leaves the set, and the session's other connections are served as before. When the last one ends the session stays open with nobody attached. Each question the turn asks from then on is refused when it is asked ([HOST-10](#HOST-10)). The turn goes on to its end: it is not cancelled, because the person who left may return, and an effect that needed an answer does not happen. Nothing is left asking a person who is no longer there. The session stays open until it is closed or the host stops. Events the turn produces meanwhile are not sent to anyone.

`verified-by: bravebot_ui_bridge::host_process::a_client_not_attached_to_a_session_cannot_act_on_it_and_may_close_it_once_it_is_left`
`verified-by: bravebot_ui_bridge::host_process::a_question_waiting_when_the_last_attached_client_leaves_is_refused_and_its_number_answers_nothing`
`verified-by: bravebot_ui_bridge::host_process::a_question_asked_after_the_last_attached_client_left_is_refused_and_the_turn_goes_on`
`verified-by: bravebot_ui_bridge::host::an_attached_connection_acts_on_a_session_and_another_may_join_it_or_close_one_left`
`verified-by: bravebot_ui_bridge::host::an_event_goes_to_every_attached_connection_and_an_addressed_one_to_the_asker_alone`
`verified-by: bravebot_ui_bridge::host_process::two_attached_clients_are_both_asked_and_the_first_answer_counts`
`verified-by: bravebot_ui_bridge::host_process::a_send_from_each_attached_client_starts_one_turn_and_the_other_is_refused_as_busy`
`verified-by: bravebot_ui_bridge::host_process::a_resync_is_sent_only_to_the_client_that_asked`
`verified-by: bravebot_ui_bridge::host_process::a_close_reaches_each_attached_client_after_the_closer_is_answered`
`verified-by: bravebot_ui_bridge::host_process::a_client_without_trust_is_not_asked_about_trust_and_cannot_answer_it`
`verified-by: bravebot_ui_bridge::host_process::a_join_without_trust_is_not_asked_and_the_next_client_with_trust_is`
`verified-by: bravebot_ui_bridge::host::a_closing_or_unknown_session_is_not_found_by_anyone`
`verified-by: bravebot_ui_bridge::host_process::two_clients_connected_at_once_each_act_on_and_hear_only_their_own_sessions`
`verified-by: bravebot_ui_bridge::host_process::a_client_leaving_while_another_is_mid_turn_leaves_the_other_served`
`verified-by: bravebot_ui_bridge::host::a_client_leaving_leaves_the_others_served`
`verified-by: bravebot_ui_bridge::host::a_connection_given_up_on_leaves_the_others_served`
`verified-by: bravebot_ui_bridge::host::an_event_of_a_session_being_opened_goes_to_the_connection_opening_it`
`verified-by: bravebot_ui_bridge::closing_tests::narrowing_a_session_with_no_work_changes_nothing`

<a id="HOST-8"></a>
### HOST-8: a request line has a limit

The host queues each line for a connection, and a thread of the connection's own writes them, so a client that reads slowly holds up no turn and no other event. A queue of 1024 lines that is full, or one holding more than 16 MiB of lines the writer has not yet taken, ends the connection, as a client that far behind is treated as gone. A line of any length is queued while the lines waiting are within 16 MiB, so a long event and its view update, written back to back, do not end a connection that is keeping up. Only each line's length is counted; nothing in it is read. It joins its sessions again with `session.attach` to be sent where they stand ([HOST-11](#HOST-11)). A write to a client that waits more than five seconds, or fails, ends the connection: requests already read are not executed after that, because nobody can hear their answers. The channel is shut when the write gives up, so the host's read of it ends and the connection is lost even if the client sends nothing more. A client that reads nothing therefore cannot stop the host reading the connection and seeing it end. The answer to a `session.close` is written before the `session.closed` that follows it.

A request line is at most 8 MiB, newline excluded. A line that reaches the limit without ending ends the connection without being answered or dispatched. A line of exactly the limit is read. Bytes that arrive without their end of line before the client closes its end are not a request and are not run.

`verified-by: bravebot_ui_bridge::host::a_line_that_does_not_end_ends_the_connection`
`verified-by: bravebot_ui_bridge::host::a_request_at_the_limit_is_read`
`verified-by: bravebot_ui_bridge::host::a_last_line_with_no_newline_is_not_run`
`verified-by: bravebot_ui_bridge::host::a_client_that_never_reads_does_not_wedge_the_host`
`verified-by: bravebot_ui_bridge::host::a_queue_past_its_byte_budget_ends_the_connection_and_one_long_line_alone_does_not`
`verified-by: bravebot_ui_bridge::host::a_client_that_keeps_up_is_sent_more_than_the_byte_budget`
`verified-by: bravebot_ui_bridge::host::a_long_event_and_its_view_update_are_queued_together`
`verified-by: bravebot_ui_bridge::host::a_write_that_gives_up_ends_the_connection_the_host_is_reading`
`verified-by: bravebot_ui_bridge::host::a_slow_client_holds_up_no_event_and_a_full_queue_ends_its_connection`
`verified-by: bravebot_ui_bridge::host_process::quit_stops_the_host_while_a_client_is_connected`
`verified-by: bravebot_ui_bridge::host_process::a_line_of_input_that_is_not_text_does_not_stop_the_host`
`verified-by: bravebot_ui_bridge::host_process::a_client_path_that_is_not_utf8_is_not_a_panic`
`verified-by: bravebot_ui_bridge::host_process::a_sessions_root_the_host_cannot_write_to_is_refused`
`verified-by: bravebot_ui_bridge::host_process::the_answer_to_a_close_comes_before_the_session_is_said_to_be_closed`
`verified-by: bravebot_ui_bridge::host_process::a_close_is_answered_before_its_session_is_closed_while_another_client_is_answered`

<a id="HOST-9"></a>
### HOST-9: a program the host starts holds no part of the channel

The host holds the near end of its client's channel while it runs the person's hooks and the programs a turn approves. None of them inherits a descriptor of the channel: the ends are close-on-exec, the channel is created and every program is started under the one gate of [RPCVIEW-8](session-view.md#RPCVIEW-8), and the client's end is closed in the host before it serves. `bravebot-host` also marks every inherited descriptor other than standard input, output and error close-on-exec at startup, so a descriptor it was started with reaches none of its programs. An unconfined hook or an MCP server that lists its own descriptors finds no socket besides any the test process standing in for the host was started holding without close-on-exec (a CI runner leaves some open); the test leaves those out. A process that a hook or an approved command leaves running after it has exited, given the host's and the client's process ids and the waiting question, holds no socket of the channel, reaches neither end through a descriptor path or a connection, and does not change what the client reads; the waiting write lands only when the client approves it. A descriptor made in two steps outside the gate, such as the sockets to a model endpoint, is not covered, as the clause linked above says.

`verified-by: bravebot_ui_bridge::host_process::an_unconfined_hook_the_host_runs_holds_no_socket`
`verified-by: bravebot_ui_bridge::control_boundary::a_program_a_hook_the_host_runs_leaves_running_cannot_reach_the_channel`
`verified-by: bravebot_ui_bridge::control_boundary::a_program_an_approved_command_the_host_runs_leaves_running_cannot_reach_the_channel`
`verified-by: bravebot_ui_bridge::host_process::a_program_handed_a_socket_lists_it`
`verified-by: bravebot_ui_bridge::host_process::a_listing_of_either_system_shows_its_sockets`
`verified-by: bravebot_ui_bridge::control_boundary::programs_a_host_starts_hold_no_end_of_the_channels_it_created`
`verified-by: bravebot_ui_bridge::control_boundary::an_mcp_server_a_host_starts_holds_no_end_of_the_channels_it_created`
`verified-by: bravebot_ui_bridge::control_boundary::a_client_holds_no_socket_of_another_clients_channel`
`verified-by: bravebot_ui_bridge::layering::every_binary_here_withholds_inherited_descriptors_at_startup`

<a id="HOST-10"></a>
### HOST-10: a question is put only to a client that can answer it

The kinds a session can be asked are the union of the answers ([HOST-1](#HOST-1)) of its attached connections, leaving out any stopped for the session ([HOST-15](#HOST-15)), counted once the session's view has started: after `session.view.start` by a connection attached to it, or `session.attach` by one joining it. A host's session starts with nobody able to answer, whichever way it was opened. A question is put to the session only if its kind is one the host offers a reply for (`confirm`, `run`, `fetch` and `ask`, from the same list as the allowlist) and an attached connection was given that answer. A write's question is the `confirm` kind. Any other question is refused when it is asked, in the shape its kind is refused in, and is not shown, not numbered and not left waiting. This holds for every kind, so a turn that asks for command output, a vouch or an MCP call does not hang.

A question that is asked is shown to every attached connection in the view, including one not given its kind, and only a connection given its kind may answer it. Another connection's answer is refused with `not_allowed` and the question stays waiting. The startup question `trust.request` is sent only to a connection given `trust`.

When a connection leaves, the union is narrowed to the answers of the connections that stay. The question waiting then is refused and shown as resolved in the view only if its kind is no longer in the union, and otherwise it stays waiting for the connections that can answer it. The startup question is not refused this way: it stays unanswered and is sent again to a connection given `trust` that joins later. When a join fails after the union was widened for the joining connection, the union is narrowed back the same way.

The check is made under the lock that registers a question, and a narrowing sets the union before it takes that lock to refuse the waiting question. A question is therefore either refused before it is registered or registered where the narrowing finds it. A connection that leaves between the two cannot leave a question waiting for nobody. A stdio client of the bridge is unaffected: it can answer every kind.

`verified-by: bravebot_ui_bridge::answering_tests::a_question_nobody_can_answer_is_refused_before_it_is_shown_for_every_kind`
`verified-by: bravebot_ui_bridge::answering_tests::only_the_kinds_an_attached_client_can_answer_are_asked`
`verified-by: bravebot_ui_bridge::answering_tests::the_loss_of_every_answerer_refuses_the_question_waiting_for_every_kind`
`verified-by: bravebot_ui_bridge::answering_tests::a_question_refused_by_the_loss_of_its_answerers_is_shown_as_resolved`
`verified-by: bravebot_ui_bridge::answerers::a_new_set_answers_every_kind_so_a_client_that_never_chose_is_unchanged`
`verified-by: bravebot_ui_bridge::answerers::only_the_kinds_named_can_be_answered_and_nobody_answers_none`
`verified-by: bravebot_ui_bridge::answerers::a_set_that_cannot_be_written_fails_and_answers_nothing`
`verified-by: bravebot_ui_bridge::allowlist::every_offered_reply_but_the_startup_answer_is_a_kind_a_client_is_asked`
`verified-by: bravebot_ui_bridge::control_tests::a_set_of_answerers_is_only_made_for_a_session_that_is_open`
`verified-by: bravebot_ui_bridge::control_tests::a_bridge_that_refuses_until_set_starts_every_session_unanswerable`
`verified-by: bravebot_ui_bridge::host_process::a_client_attached_with_its_view_started_is_asked_and_its_approval_lands`
`verified-by: bravebot_ui_bridge::host_process::a_session_whose_client_never_started_a_view_has_its_questions_refused_unseen`
`verified-by: bravebot_ui_bridge::allowlist::a_client_is_asked_only_the_kinds_it_may_answer`
`verified-by: bravebot_ui_bridge::host_process::a_write_is_not_put_to_a_client_that_may_not_confirm_it`
`verified-by: bravebot_ui_bridge::answering_tests::narrowing_refuses_the_waiting_question_only_when_its_kind_is_left_out`
`verified-by: bravebot_ui_bridge::answering_tests::narrowing_sets_who_can_answer_before_it_looks_for_a_question_to_refuse`
`verified-by: bravebot_ui_bridge::control_tests::narrowing_to_kinds_without_the_waiting_question_refuses_it_and_does_not_stop_the_turn`
`verified-by: bravebot_ui_bridge::control_tests::narrowing_to_kinds_that_cover_the_waiting_question_leaves_it_waiting`
`verified-by: bravebot_ui_bridge::host_process::a_client_leaving_while_another_attached_can_answer_leaves_the_question_waiting`
`verified-by: bravebot_ui_bridge::host_process::a_client_leaving_that_alone_could_answer_the_waiting_question_refuses_it`

<a id="HOST-11"></a>
### HOST-11: a client that joins a session is told where it stands

`session.attach` adds the connection to the attached set of an open session that has started its view ([HOST-7](#HOST-7)), whether or not other connections are attached, and answers `{ "version": 1, "sequence": n, "omitted": k }`. It is refused for a connection already attached, a session that started no view, one that is closing or closed, and one the host does not hold.

Before it answers, the joining connection alone is sent `session.view.snapshot` ([RPCVIEW-9](session-view.md#RPCVIEW-9)): the session's current sequence, turn, turn target, status and pending question, its most recent rows including those produced while no client was connected, and `omitted`, how many earlier rows it leaves out. The connection is added to the set and the snapshot is written as one step with respect to every update, so each update the joining connection receives has a sequence after the snapshot's, each connection already attached receives every update once, and none is missed or repeated. A session whose startup question is unanswered is still `awaiting_trust` in the snapshot, and the question is sent again after it to the joining connection if it was given `trust`. A connection not given it is not sent the question, and neither is a client given `confirm` only that opens a session: its view says `awaiting_trust`. Trust is not answered by joining the session, and no turn runs until a connection given `trust` answers it.

The joining connection's answers count toward the kinds of [HOST-10](#HOST-10) from the snapshot on. A question refused because the last connection that could answer it left is resolved, and a later question has a number the earlier one did not. An answer naming the earlier question's number is refused with `no_such_request` and changes nothing, as is a cancel naming a target that has ended.

`verified-by: bravebot_ui_bridge::host_process::a_question_waiting_when_the_last_attached_client_leaves_is_refused_and_its_number_answers_nothing`
`verified-by: bravebot_ui_bridge::host_process::a_question_asked_after_the_last_attached_client_left_is_refused_and_the_turn_goes_on`
`verified-by: bravebot_ui_bridge::host_process::a_session_left_with_its_startup_question_unanswered_asks_the_next_client`
`verified-by: bravebot_ui_bridge::host_process::a_client_not_attached_to_a_session_cannot_act_on_it_and_may_close_it_once_it_is_left`
`verified-by: bravebot_ui_bridge::host_process::a_session_whose_client_never_started_a_view_has_its_questions_refused_unseen`
`verified-by: bravebot_ui_bridge::control_tests::joining_a_session_attaches_between_the_last_update_and_the_snapshot`
`verified-by: bravebot_ui_bridge::host_process::a_client_that_returns_is_sent_the_rows_it_missed_and_each_update_after_them`
`verified-by: bravebot_ui_bridge::control_tests::the_startup_question_is_asked_again_only_while_it_is_unanswered`
`verified-by: bravebot_ui_bridge::control_tests::a_session_that_cannot_be_joined_attaches_nobody`
`verified-by: bravebot_ui_bridge::control_tests::a_hand_over_that_fails_sends_nothing_and_leaves_who_can_answer_as_it_was`
`verified-by: bravebot_ui_bridge::control_tests::updates_during_a_join_fall_on_one_side_of_the_snapshot`
`verified-by: bravebot_ui_bridge::host_process::a_join_between_two_updates_misses_none_and_repeats_none_for_either_client`
`verified-by: bravebot_ui_bridge::host::an_attached_connection_acts_on_a_session_and_another_may_join_it_or_close_one_left`

<a id="HOST-12"></a>
### HOST-12: the development relay hands the channel to one app on loopback, behind a code

The development relay is a client program for the host, named on its command line as one client (`--client relay --answers KINDS --program PATH`), with its script and the same `--workspace` directories the host was given passed as `--arg` values. The person types `launch` and its name to start it again. It is for development only: any program on the computer that can reach 127.0.0.1 can reach it, and the code is the only thing between them. It does not identify a phone or a person and does not authorize control of a real session, so it is run with synthetic data only. The host treats it as any other client: every rule above applies to what arrives through it.

It listens on 127.0.0.1 only, on a port the system picks, and shows the port and a six-digit code drawn for that launch on standard error. Nothing writes the code anywhere else, and the code is not part of the address. It takes one WebSocket connection per launch, and stops listening once that connection arrives, so a second app cannot reach it. The first message on the connection must be the code. A wrong code closes the connection with code 4001, and no code within a minute closes it too. Either ends that relay. After the right code it sends the list of its workspaces, then passes text both ways without reading it. A binary message, an unmasked frame, a reserved bit, a fragmented control frame, text that is not UTF-8, or a message over 32 MiB and 4 bytes (the host's 8 MiB line limit at four bytes a character) closes the connection, and nothing of it is passed on. Either end closing ends the relay, which ends the host's channel and leaves the host running. The next launch has a port and code of its own.

The TypeScript client reaches the relay only through its `./dev-relay` export, which the package's main entry does not import. It refuses a port outside 1 to 65535 or a code that is not six digits before it opens anything, and reports a wrong code apart from a relay that could not be reached.

`verified-by: by-construction (packages/agent-client/tests/dev-relay.test.ts starts a built bravebot-host with the relay as its client against a model service of its own, and checks that an app giving the code drives a session to an approved write that lands, that a wrong code ends that relay and leaves the host running and the next launch has a new port and code, that a connection giving no code within the wait is closed and ends the relay while one giving the code in time stays open past it, with the wait shortened by BRAVEBOT_DEV_RELAY_CODE_WAIT_MS for the test, that a second app cannot connect while the first is connected, that the app closing ends the relay and the host quitting ends the app's connection, and that a bad port or code opens nothing; packages/agent-client/tests/websocket.test.ts checks fragment reassembly, ping, and the close code for each frame the relay refuses; make check-agent-client runs both, and the Front end CI job runs that target)`

<a id="HOST-13"></a>
### HOST-13: the host can be found by no name

Started without `--listen`, the host listens on no TCP port, binds no UDP port, and binds no Unix socket to a path or, on Linux, to an abstract name. A program the host did not start therefore has no address to reach it by. Started with `--listen`, it listens on that one address and holds no other socket of any of those kinds, so the only address a program can reach is the one the person named, and what it reaches serves a device the person paired and nobody else ([LISTEN-2](device-listener.md#LISTEN-2)). The development relay of [HOST-12](#HOST-12) listens on loopback in a client process the host starts, not in the host. The test lists the sockets the host holds after it has served a turn, so a socket it opened and closed before then, or opens only on a later request, is not seen. On Linux the test does not count a Unix socket that is connected, since its peer is the only program that can reach it.

`verified-by: bravebot_ui_bridge::host_process::a_host_serving_a_turn_has_no_socket_another_program_can_find`
`verified-by: bravebot_ui_bridge::host_process::a_host_given_listen_holds_that_one_address_and_no_other_socket_a_program_can_find`

<a id="HOST-14"></a>
### HOST-14: the host test client is a client the person types into

The host test client is a client program for the host, named on its command line as one client (`--client NAME --answers KINDS --program PATH`) with its script, its own name, the one workspace it may open and, where it is not to read the terminal, the file it takes its commands from passed as `--arg` values. It lists the sessions the host holds and whether it is attached to each, opens a session in its workspace, joins a session another client opened, sends, cancels, closes, and answers what it is shown. It shows the startup trust question as a typed yes or no only when the host let it answer trust, and says where it may not that the question is answered on the computer the host runs on. It shows a question a turn asks and takes `approve` or `reject` only for a kind it may answer, and marks any other as one to answer on another device. A command that the host refuses is reported with the host's code and does not end the client. It writes what it sees to standard error, prefixed with its name, with every control and invisible character escaped, and holds no credential or setting. It is for tests and development with synthetic data. It is a client like any other: every rule above applies to what it sends.

`verified-by: by-construction (packages/agent-client/tests/host-clients.test.ts starts a built bravebot-host with two of these clients and a model service of its own, and checks that a client given trust is shown the startup question and answers it while a client without trust is not shown it and has its answer refused, that a question is shown to both clients and the first answer lands while a later answer from the other is refused, that a client leaving keeps the question for the one that stays and a question a client may not answer is marked for another device, and that a second send while a turn runs is refused as busy; tests/dev-relay.test.ts answers the startup question of a relay's session from this client)`

<a id="HOST-15"></a>
### HOST-15: a connection held to a ceiling is stopped before a session it is attached to goes past it

A connection can be served held to a ceiling, the scope a person approved for it ([decision 18](../design/mobile/decisions-and-tradeoffs.md#18-a-devices-scope-is-a-ceiling-set-at-pairing)). No client the host starts is held to one. It is how a paired device's connection is to be served, and no device's connection reaches the host yet ([LISTEN-6](device-listener.md#LISTEN-6)).

The bridge tells the host of each moment a session can come to reach more, before it sends anything that follows:

- when a turn starts, with the scope the session holds then, which takes in a settings edit made since the last turn;
- when the trust question is answered, with the scope the answer leaves;
- when the person approves, for the turn, a fetch of a web host or a line run with no sandbox, which no ceiling holds.

A scope that cannot be read is not taken to be inside a ceiling. A write the person approves is inside the session's workspace, and a line run confined is held to the sandbox the scope names, so neither stops a connection.

At each of these moments, each attached connection whose ceiling does not cover the session, as `Scope::covers` decides, is stopped for that session:

- it is sent `{"event":"session.suspended","session":S,"data":{"widened":[...]}}`, which names what went past, in the words `Field::name` gives;
- it is sent no other event of the session from then on, except `session.closed`;
- every request it makes about the session is refused with `not_allowed`;
- it counts for no kind of question in the session ([HOST-10](#HOST-10)).

A narrowing stops no connection, and a stopped connection is not told twice. Who can answer is worked out again wherever the host set it from what it read before a stop a turn's thread made, so a question is never put to a stopped connection alone. A connection held to a ceiling is also refused `session.new` in a workspace, and `session.attach` of a session, whose scope its ceiling does not cover, before anything of the session reaches it. For a join, the scope is the session's now where a turn does not hold its state, and otherwise the one the host was last told of, and a session in which a fetch or an unconfined line was approved is refused for as long as it is open, since its view goes on holding what the grant brought in. A grant is recorded, and the connections attached are read, in one hold of the lock a join is handed over under, so a grant approved as a connection joins either refuses the join or stops the connection. When a session closes, what the host held about it goes.

The person approves a stopped connection for a session on the host, through `Host::approve`, which no command typed on the host calls yet. The connection is held in that session alone to the scope the session holds then, or, while a turn holds the session's state, the one the host was last told of; where that could not be read, the approval is refused until it can. Its ceiling does not change, so every other session, and every session it opens or joins later, is still measured against the ceiling set at pairing, and an approval neither widens nor stops another session. What was approved for a session goes when the session closes. An approval covers what the session held when the stop is lifted, and a grant approved after that stops the connection again. The stop is lifted where a join is handed over, and the connection is sent the session's view again ([HOST-11](#HOST-11)) before any later event, and can act and answer in the session as before.

`verified-by: bravebot_ui_bridge::host::a_held_connection_is_stopped_before_what_follows_a_widening`
`verified-by: bravebot_ui_bridge::host::a_covered_scope_stops_nobody_and_one_that_could_not_be_read_stops_a_held_connection`
`verified-by: bravebot_ui_bridge::host::a_grant_for_one_turn_stops_every_held_connection_and_no_other`
`verified-by: bravebot_ui_bridge::host::a_stop_before_the_view_names_no_kinds`
`verified-by: bravebot_ui_bridge::host::a_stopped_connection_acts_on_nothing_until_the_person_approves_it`
`verified-by: bravebot_ui_bridge::host::a_held_connection_opens_no_session_past_its_ceiling`
`verified-by: bravebot_ui_bridge::host::an_approved_connection_is_sent_the_view_again_and_answers_as_before`
`verified-by: bravebot_ui_bridge::host::a_held_connection_joins_no_session_a_grant_for_one_turn_widened`
`verified-by: bravebot_ui_bridge::host::a_join_is_checked_again_for_a_grant_where_it_is_handed_over`
`verified-by: bravebot_ui_bridge::host::kinds_set_from_before_a_stop_are_settled_to_leave_the_stopped_connection_out`
`verified-by: bravebot_ui_bridge::host::every_kind_a_host_client_can_answer_is_decided_for_the_ceiling`
`verified-by: bravebot_ui_bridge::host::approving_is_refused_while_the_sessions_scope_cannot_be_read`
`verified-by: bravebot_ui_bridge::host::an_approval_for_one_session_does_not_widen_another`
`verified-by: bravebot_ui_bridge::host::an_approval_for_one_session_does_not_stop_another`
`verified-by: bravebot_ui_bridge::answerers::a_change_made_while_kinds_are_worked_out_lands_after_them`
`verified-by: bravebot_ui_bridge::answering_tests::an_approved_fetch_tells_the_scope_watch_before_the_yes_is_acted_on`
`verified-by: bravebot_ui_bridge::answering_tests::an_approved_unconfined_run_tells_the_scope_watch_and_a_confined_one_does_not`
`verified-by: bravebot_ui_bridge::scope_tests::the_trust_answer_is_told_to_the_scope_watch_before_anything_it_makes_the_bridge_say`
`verified-by: bravebot_ui_bridge::scope_tests::a_turn_is_told_to_the_scope_watch_before_anything_of_it_is_sent`

<a id="HOST-16"></a>
### HOST-16: a connection held to a ceiling is sent a device's view

A connection held to a ceiling ([HOST-15](#HOST-15)) is how a paired device is to be served, and the person agreed at pairing that the device may show what a session shows them, except what came from somewhere nobody vouched for ([decision 19](../design/mobile/decisions-and-tradeoffs.md#19-a-paired-device-is-an-approved-screen-that-receives-an-allowlist-of-events)). Such a connection is sent only `agent.ready`, `session.view.initial`, `session.view.update`, `session.view.snapshot`, `session.suspended`, `session.closed` and `trust.request`, the last only where it was given `trust`. Every other event is not sent to it, so an event the bridge comes to emit reaches no device until it is named here. A connection held to no ceiling is sent every event whole.

In each view it is sent, live or as a snapshot, these rows keep their id, turn, kind, event and resolution and have their content replaced with `{"withheld":true}`:

- a quarantined preview;
- a tool's activity the agent marked `untrusted`, whose target came from content nobody vouched for;
- a question a device is not offered, whose placeholder also says `"answer":"on the computer"`.

A device is offered a `confirm`, `run`, `fetch` or `ask` question, and not one of any other kind, which by what it is holds content nobody vouched for, such as a command's output or a file to vouch for. Nor is it offered one the agent marked `untrusted`, a write whose body came from such content, a write with an isolated processor's remark, which is always untrusted, or a fetch a redirect led to, whose address a remote server chose. Only the bridge's own row kind and question kind, a flag set to `true`, and whether the remark and the redirect are there decide this, and no text of a row is read.

A question a device is not offered is also left out of what waits in the connection's view, and the connection's answer to it is refused with `not_allowed`. Such a write or fetch is put to the session only where a connection held to no ceiling can answer it, checked under the lock that registers the question, and is otherwise refused when it is asked, unshown and unnumbered, as a question nobody can answer is ([HOST-10](#HOST-10)). When the last connection held to no ceiling that could answer it leaves while it waits, it is refused and shown as resolved, so it does not wait on a device that is not shown it. A view that cannot be read as one is not sent to the connection, rather than sent whole. What the connection was not offered is forgotten when the session closes.

`verified-by: bravebot_ui_bridge::device_view::what_nobody_vouched_for_reaches_a_device_as_a_placeholder_in_its_place`
`verified-by: bravebot_ui_bridge::device_view::a_question_nothing_marked_is_offered_to_a_device`
`verified-by: bravebot_ui_bridge::device_view::only_the_flag_the_agent_sets_withholds_a_row`
`verified-by: bravebot_ui_bridge::device_view::a_device_is_sent_only_these_events`
`verified-by: bravebot_ui_bridge::host::a_held_connection_is_sent_a_devices_view`
`verified-by: bravebot_ui_bridge::host::only_a_connection_that_is_no_device_answers_locally`
`verified-by: bravebot_ui_bridge::answering_tests::an_untrusted_write_is_refused_unasked_where_only_a_device_could_answer_it`
`verified-by: bravebot_ui_bridge::answering_tests::a_question_only_a_local_connection_may_answer_is_refused_when_the_last_one_goes`
`verified-by: bravebot_ui_bridge::answering_tests::a_redirected_fetch_is_refused_unasked_where_only_a_device_could_answer_it`
`verified-by: bravebot_ui_bridge::device_view::each_question_a_device_is_not_offered_is_withheld`
`verified-by: bravebot_ui_bridge::host::a_closed_session_leaves_nothing_withheld_stopped_or_approved`

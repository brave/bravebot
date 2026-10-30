---
id: DIAG
title: The diagnostic log
status: normative
governs:
  - crates/diag/src/lib.rs
  - crates/diag/src/report.rs
  - crates/cli/src/bug_report.rs
  - crates/ui-bridge/src/bug_report.rs
  - crates/ui-bridge/src/bin/bravebot-rpc.rs
  - ui/src/main/bug-report.ts
  - ui/src/renderer/components/BugReport.tsx
  - crates/ui-bridge/src/window_fault.rs
  - ui/src/main/window-fault.ts
  - ui/src/renderer/fault.ts
  - ui/src/renderer/components/WindowFault.tsx
documented-by: docs/website/docs/reference/cli.md
---

## Scope

A file a person can attach to a bug report to say what went wrong on their machine: which host a
request failed against, with which status, how many attempts were made, whether a language server
or an MCP server started. It is for a human reading afterwards.

It is not [trace.md](trace.md). The trace is the audit trail of gate decisions and is part of the
session's record. This log is about the program's own failures and steps, is kept apart from any
session, and holds no decision a gate made.

## Clauses

<a id="DIAG-1"></a>
### DIAG-1: a line holds a host, a number or a fixed word, and nothing else

The log has no way to write a string of unknown origin. A host is taken from a URL with its
userinfo, path, query and fragment dropped, and is written as `?` if it holds anything that is not
a host character. Everything else is a count or a word fixed in the source. So a request or reply
body, a prompt, a file's content, a header value, a credential, a server's name for itself and the
text of a server's error are never written, and a byte a remote party chose cannot start a new
line.

**Why.** A person is asked to attach this file to a public issue. A log that kept the text of a
failure would carry a token from a URL, or a sentence a server composed, into a place nobody
reviewed. Holding it by what a field can be, rather than by each call site's care, is what makes
it true of the call site written next year.

`verified-by: bravebot_diag::lib::a_host_field_drops_userinfo_path_and_query`
`verified-by: bravebot_diag::lib::a_host_with_odd_bytes_is_written_as_a_question_mark`
`verified-by: bravebot_diag::lib::a_host_with_an_underscore_is_kept`
`verified-by: bravebot_net::lib::a_failed_request_is_logged_as_host_status_and_kind_only`
`verified-by: bravebot_cli::running::a_failed_request_leaves_its_host_and_status_and_no_content_in_the_log`

<a id="DIAG-2"></a>
### DIAG-2: a run's log is a private file under the state directory, and the oldest are removed

Each process writes its own file under `logs/` in the state directory, readable by its owner alone
in a directory readable by its owner alone. A `logs/` that is a link is stepped over rather than
followed, as every writer into the state directory steps over one, so the directory it leads to
keeps its mode and the file written there is still its owner's alone. Ten files are kept: making a
new one removes the oldest of those this program made, never the one just made, and leaves any
other file in the directory alone.

**Why.** The directory grows with every run otherwise, and the files name the hosts a person
talks to, which is theirs to share and nobody else's to read.

`verified-by: bravebot_diag::lib::the_log_and_its_directory_are_private`
`verified-by: bravebot_diag::lib::narrowing_does_not_follow_a_linked_log_directory`
`verified-by: bravebot_diag::lib::retention_keeps_the_newest_and_leaves_foreign_files`
`verified-by: bravebot_diag::lib::the_file_being_written_survives_retention_when_the_clock_is_behind`
`verified-by: bravebot_cli::running::a_failed_request_leaves_its_host_and_status_and_no_content_in_the_log`

<a id="DIAG-3"></a>
### DIAG-3: `--log-level` chooses how much is written, and a run that has nothing to report leaves no file

The levels are `error`, `info` and `debug`, each including the ones before it, and `error` is the
default. The file is made by the first line written to it, so at the default level a run that
fails nowhere leaves nothing behind. A word that is not a level is refused as an argument and the
run does not start.

**Why.** Most runs have nothing worth keeping, and a directory of empty files is noise. A
mistyped level read as the default would leave a person without the log they asked for, found out
only after the failure they wanted it for.

`verified-by: bravebot_diag::lib::a_line_above_the_level_is_not_written`
`verified-by: bravebot_diag::lib::no_file_is_made_until_a_line_is_written`
`verified-by: bravebot_cli::main::the_log_level_flag_is_taken_out_with_its_word_and_refuses_any_other`
`verified-by: bravebot_cli::running::a_log_level_that_is_not_one_is_refused`
`verified-by: bravebot_cli::running::the_level_on_the_command_line_reaches_the_log_and_the_default_is_error`

<a id="DIAG-4"></a>
### DIAG-4: an incognito session, or a machine with no home, writes no log

Neither creates the directory or a file in it. The desktop app's agent, `bravebot-rpc`, writes its
log where the terminal does, at the default level, and to nowhere on the same two conditions.

**Why.** An incognito session adds nothing to the state directory, and a log that said where it
had been would be the trace the mode exists to avoid.

`verified-by: bravebot_diag::lib::a_log_with_no_directory_writes_nothing`
`verified-by: bravebot_cli::running::an_incognito_session_writes_no_log`
`verified-by: bravebot_ui_bridge::diagnostic_log::a_desktop_turn_that_cannot_reach_its_service_leaves_a_log`
`verified-by: bravebot_ui_bridge::no_state_directory::with_no_state_directory_there_is_no_bug_report_and_no_update_record`

<a id="DIAG-5"></a>
### DIAG-5: `doctor` names the directory the logs are in

**Why.** Asking somebody to attach the log is no use if they cannot be told where it is, and the
state directory moves with the profile variable, so the path is not one to be assumed.

`verified-by: bravebot_cli::doctor::doctor_names_the_diagnostic_log_directory`
`verified-by: bravebot_cli::running::doctor_names_the_log_directory`

<a id="DIAG-6"></a>
### DIAG-6: nothing in the program reads the log

No code path opens a log file to decide, show or send anything. The log is written and left for a
person. The one question asked of the directory is the name of the newest file, answered from the
names alone ([DIAG-8](#DIAG-8)).

**Why.** The planner and the driver never receive untrusted content and the driver does not branch
on it. A log that was read back would be a channel from a remote party's failure into the next
decision, whatever the log was meant for.

`verified-by: by-construction (the crate exposes functions to write a line and to name the newest file, and none to read a file's content; the directory name is read by the CLI only to be passed to the writer, to be shown by doctor and to be handed to the naming function, and by the desktop bridge only to be handed to the naming function)`

<a id="DIAG-7"></a>
### DIAG-7: a failed request, a retry and a server that did not start are recorded

A request that failed names its host, the kind of failure, the status where there was one and how
long it took, counted from when the request began. A reply that fails while it is still being read,
by a reset, a cut stream or a deadline passing, is a request that failed, whichever way the reply
is read, and names the status it began with. A decision to send a request again names which attempt it is and the wait before it.
Starting a language server or an MCP server, and an MCP handshake over either transport, are
recorded as having worked or as the kind of failure, and a language server's exit as an orderly one
or a kill. At the default level only failures are written, and a request a person stopped is not
one, nor is a read that fails after the person stopped it.

**Why.** These are the questions a bug report starts with: could it reach the service, was it
retried, did the server it needs start. Each is a fact about the program and none needs content.

**Known costs.** A reply a caller gave up on without a stop, because it had already failed on a
frame it read, can still be waiting on its connection, and when that gives out the log holds a
second failure for a request that was already reported. A request a person stopped is not written
however its read then ends, including a connection that failed after the stop. A refusal whose body
failed while [NET-12](network-egress.md#NET-12) read it is written twice, as the body failing and as the
status that is the failure.

`verified-by: bravebot_net::lib::a_failed_request_is_logged_as_host_status_and_kind_only`
`verified-by: bravebot_net::lib::a_stopped_request_is_not_a_failure_to_log`
`verified-by: bravebot_net::lib::a_read_that_is_repeated_is_not_written_as_a_body_that_failed`
`verified-by: bravebot_net::egress::a_request_whose_body_fails_while_it_is_read_is_written_to_the_diagnostic_log`
`verified-by: bravebot_net::egress::a_reply_that_runs_past_its_deadline_is_written_as_out_of_time`
`verified-by: bravebot_net::egress::a_body_failure_is_timed_from_when_the_request_began`
`verified-by: bravebot_net::egress::a_body_that_fails_after_the_request_was_stopped_is_not_written`
`verified-by: bravebot_net::egress::a_refusal_whose_body_fails_while_it_is_read_is_written_to_the_diagnostic_log`
`verified-by: bravebot_aichat::client::a_reply_cut_off_part_way_is_written_to_the_diagnostic_log_as_well_as_its_retries`
`verified-by: bravebot_aichat::client::a_retry_is_written_to_the_diagnostic_log`
`verified-by: bravebot_bedrock::lib::a_retry_is_written_to_the_diagnostic_log`
`verified-by: bravebot_mcp::stdio::a_launch_that_failed_is_written_to_the_diagnostic_log`
`verified-by: bravebot_mcp::stdio::a_handshake_is_written_to_the_diagnostic_log`
`verified-by: bravebot_mcp::http_log::a_handshake_is_written_to_the_diagnostic_log_when_it_works`
`verified-by: bravebot_mcp::http_log::a_handshake_that_failed_is_written_as_the_kind_of_failure_at_the_default_level`
`verified-by: bravebot_mcp::http_log::a_handshake_that_works_writes_nothing_at_the_default_level`
`verified-by: bravebot_lsp::server::a_missing_server_binary_is_written_to_the_diagnostic_log`
`verified-by: bravebot_lsp::server::a_servers_launch_and_exit_are_written_to_the_diagnostic_log`

<a id="DIAG-8"></a>
### DIAG-8: `bug-report` writes one private file naming what to attach, and copies no log

`bravebot bug-report` writes `bravebot-bug-report.txt` in the current directory, readable by its
owner alone, and prints its path on stdout. A name already taken is left as it is and the next
number is tried, so an earlier report is never overwritten. The file holds the build and the
target, what `doctor` writes on each stream and the path of the newest diagnostic log, taken from
the file names before `doctor` runs, since `doctor` may write a log of its own. It holds no
transcript, trace, settings file, environment value, file of the working directory or line of a
log. An incognito session, or a machine with no home, writes nothing and exits with status 1. An
argument is refused with status 2 and nothing is written.

**Why.** A person asked to attach something to a public issue is helped by a file that says what
the program is and how it is set up, and harmed by one that carries what they were working on.
Running `doctor` as the program itself started again makes the file the report the person would
otherwise have copied from their terminal. Copying the log's lines would be a read of a log
(DIAG-6), so the path is given and the person attaches the file.

**Known costs.** `doctor` names paths, model services and the names of environment variables, so
the file is for a person to read before it is posted. Two files are attached rather than one.

`verified-by: bravebot_cli::running::a_bug_report_holds_the_build_doctors_report_and_the_logs_name_and_no_content`
`verified-by: bravebot_cli::running::a_second_bug_report_does_not_overwrite_the_first`
`verified-by: bravebot_cli::running::a_bug_report_in_an_incognito_session_writes_nothing`
`verified-by: bravebot_cli::running::a_bug_report_with_an_argument_is_refused`
`verified-by: bravebot_cli::bug_report::a_report_never_overwrites_one_and_is_private`
`verified-by: bravebot_cli::bug_report::the_report_holds_the_build_both_streams_and_the_log_path`
`verified-by: bravebot_cli::bug_report::the_report_is_the_layout_it_has_always_been`
`verified-by: bravebot_diag::lib::the_newest_log_is_the_greatest_timestamp_among_files_this_crate_made`
`verified-by: bravebot_diag::report::a_report_is_its_header_its_sections_and_the_logs_path`

<a id="DIAG-9"></a>
### DIAG-9: the desktop app's bug report is the same file, made from what Run Diagnostics shows

Help, Report a Bug… and About's Report a bug link open a preview of the report's exact text, which
`bug-report.compose` builds in the layout DIAG-8's file has: a header naming the desktop app, its
interface version, the agent's build and the target, the settings report Run Diagnostics shows in
place of `doctor`'s output, and the path of the newest log, taken from the file names before
anything else the request does. The interface version is written only where it is a version's
shape, and as `unknown` otherwise. Above the preview a note says what the text names and what it
holds none of. Save Report… writes it where the person chooses, `bravebot-bug-report.txt` in
Documents by default, readable by its owner alone at every moment of the write: it goes to a new
owner-only file beside the one chosen, renamed over it. The main process asks the agent for the text
again rather than taking the window's copy. Copy puts it on the clipboard. Open an Issue opens the
repository's new issue page, a literal with nothing in its query. `bug-report.compose` names the
log's path only where it is text, and no log otherwise; which file is the newest log is the agent's
rule alone. Show Log reveals that log in the file manager, only where the main process finds the
path is still a plain file, and never opens it.
An incognito session, or a machine with no home, is told a report is not available, and nothing is
offered to save. It holds nothing the terminal's file holds none of, and not the agent's standard
error either.

**Why.** The app bundle has no command-line binary to run `doctor` with, and the settings report is
what the window already shows a person asking it about their setup, so it is the same answer in
the same place. One format means a maintainer reads either report the same way. A preview makes the
DIAG-8 cost, a file to read before posting, something a person sees before saving rather than
after. Asking the agent again at save time means the renderer cannot choose what is written. An
issue page with a body in its address would put the report in browser history and server logs
before the person read it. The agent's standard error carries notices, refusals and panic messages
that hold none of DIAG-1's discipline. Writing in place and then narrowing the mode would leave the
report readable by others in between, where the file chosen was. A path converted lossily would
name a file other than the one found, and a second copy of the rule for a log's name in the main
process is one to keep in step with the agent's.

**Known costs.** The settings report names paths, model services and credential states by kind,
so the note asks the person to read it before posting. The report is shown, saved and posted by
hand: nothing sends it. A state directory whose path is not text offers no Show Log, though the
report's text still names the log as the system prints it. Saving over a file replaces it, so a
link at the chosen path is replaced rather than written through.

`verified-by: bravebot_ui_bridge::bug_report::the_desktop_report_names_both_versions_the_settings_report_and_the_log`
`verified-by: bravebot_ui_bridge::bug_report::an_interface_version_of_another_shape_is_not_written`
`verified-by: bravebot_ui_bridge::bug_report::the_report_names_the_build_the_settings_file_and_the_log_and_holds_neither`
`verified-by: bravebot_ui_bridge::bug_report::an_interface_version_of_another_shape_is_unknown_and_no_log_is_none`
`verified-by: bravebot_ui_bridge::bug_report::a_log_path_that_is_text_is_sent`
`verified-by: bravebot_ui_bridge::bug_report::a_log_path_that_is_not_text_is_sent_as_no_log`
`verified-by: bravebot_ui_bridge::no_state_directory::with_no_state_directory_there_is_no_bug_report_and_no_update_record`
`verified-by: by-construction (the main process and the renderer are not crates this workspace compiles, so ui/scripts/bug-report.test.mjs pins the window's half: compose states the app's own version as the interface and holds no rule for a log's name, a save composes again after the sheet closes and writes that text with mode 0600 into a new file renamed over one that was 0644, leaving nothing beside it, and a save that cannot be written fails and leaves no file, a cancelled sheet writes nothing and an unavailable report fails, only the two named pages open and at their literal URLs, the log is shown only where it is still a plain file and never through a link, a directory or a relative path, the modal carries the privacy note and hands the main process no text, URL or path, and bug-report.compose is not on the window's allow-list; make check-ui runs it, and the governs list above holds the files to existing)`

<a id="DIAG-10"></a>
### DIAG-10: a failure of the desktop window's page, or of a process behind it, is recorded as fixed words

When React meets an error while it draws, when an error or a rejected promise reaches the window
with nothing to catch it, or when Electron reports that the window's process or another process of
the app is gone, the main process asks the agent to write one `window.fault` line. The line holds
the kind of failure and, where the report has them, the name of the error, Electron's reason, which
process, and an exit code. The agent chooses each word from a list fixed in its source, and a
string that is on no list is written as `other`. The window sends the error's name and nothing
else. The main process forwards only the five fields the agent reads, and at most twenty reports in
a run. The line is written at every level, and not at all in an incognito session or on a machine
with no home ([DIAG-4](#DIAG-4)).

When React meets the error, the window replaces the page with a screen that shows the error's name
and message, the components React was drawing, and its stack, with Reload and Copy details. They are
drawn as text and are sent nowhere. Reload loads the page again through the main process, since the
window refuses every navigation.

**Why.** A page that fails leaves a black window, and nothing to attach to a bug report. The
components React was drawing name the one that failed, which no line in this log can say, because a
message and a stack hold whatever the page was drawing ([DIAG-1](#DIAG-1)). Showing them to the
person leaves it to the person to read them before posting. Fixed words let the log say that a
failure happened and what kind, without the text.

**Known costs.** The log does not say which component failed; the screen does. A failure the main
process never hears of, and a main process with no agent running, leave no line. Past twenty
reports in a run, further reports are dropped.

`verified-by: bravebot_ui_bridge::window_fault::a_page_error_is_written_as_its_kind_and_the_name_of_the_error`
`verified-by: bravebot_ui_bridge::window_fault::a_process_that_is_gone_is_written_with_why_and_its_exit_code`
`verified-by: bravebot_ui_bridge::window_fault::a_process_name_with_a_space_is_written_without_one`
`verified-by: bravebot_ui_bridge::window_fault::a_string_that_is_on_no_list_is_written_as_other_whatever_it_says`
`verified-by: bravebot_ui_bridge::window_fault::a_field_that_is_not_text_or_a_count_is_left_out`
`verified-by: bravebot_ui_bridge::window_fault::a_request_with_no_kind_is_refused`
`verified-by: bravebot_ui_bridge::diagnostic_log::a_window_fault_is_logged_as_fixed_words_and_nothing_the_window_sent`
`verified-by: by-construction (the main process and the renderer are not crates this workspace compiles, so ui/scripts/window-fault.test.mjs pins the window's half: the main process forwards the five fields and drops what else a page adds, cuts a long string, forwards an exit code only where it is a count, and stops at twenty reports; the page names an error by its name and sends the main process that and nothing else; the screen draws the error's message as text so markup in it stays inert; and ui/scripts/drive-window-fault.mjs makes the browser refuse one removeChild in a real window with a real agent and a scratch state directory, and asserts that the screen appears with the components named, that one line holding the kind and the error name and not the message is written, and that Reload brings the app back; make check-ui runs the first, and the governs list above holds the files to existing)`

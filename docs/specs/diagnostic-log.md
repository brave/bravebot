---
id: DIAG
title: The diagnostic log
status: normative
governs:
  - crates/diag/src/lib.rs
  - crates/cli/src/bug_report.rs
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
in a directory readable by its owner alone. Ten files are kept: making a new one removes the oldest
of those this program made, never the one just made, and leaves any other file in the directory
alone.

**Why.** The directory grows with every run otherwise, and the files name the hosts a person
talks to, which is theirs to share and nobody else's to read.

`verified-by: bravebot_diag::lib::the_log_and_its_directory_are_private`
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

<a id="DIAG-4"></a>
### DIAG-4: an incognito session, or a machine with no home, writes no log

Neither creates the directory or a file in it.

**Why.** An incognito session adds nothing to the state directory, and a log that said where it
had been would be the trace the mode exists to avoid.

`verified-by: bravebot_diag::lib::a_log_with_no_directory_writes_nothing`
`verified-by: bravebot_cli::running::an_incognito_session_writes_no_log`

<a id="DIAG-5"></a>
### DIAG-5: `doctor` names the directory the logs are in

**Why.** Asking somebody to attach the log is no use if they cannot be told where it is, and the
state directory moves with the profile variable, so the path is not one to be assumed.

`verified-by: bravebot_cli::main::doctor_names_the_diagnostic_log_directory`
`verified-by: bravebot_cli::running::doctor_names_the_log_directory`

<a id="DIAG-6"></a>
### DIAG-6: nothing in the program reads the log

No code path opens a log file to decide, show or send anything. The log is written and left for a
person. The one question asked of the directory is the name of the newest file, answered from the
names alone ([DIAG-8](#DIAG-8)).

**Why.** The planner and the driver never receive untrusted content and the driver does not branch
on it. A log that was read back would be a channel from a remote party's failure into the next
decision, whatever the log was meant for.

`verified-by: by-construction (the crate exposes functions to write a line and to name the newest file, and none to read a file's content; the directory name is read by the CLI only to be passed to the writer, to be shown by doctor and to be handed to the naming function)`

<a id="DIAG-7"></a>
### DIAG-7: a failed request, a retry and a server that did not start are recorded

A request that failed names its host, the kind of failure, the status where there was one and how
long it took. A decision to send a request again names which attempt it is and the wait before it.
Starting a language server or an MCP server, and an MCP handshake, are recorded as having worked or
as the kind of failure, and a language server's exit as an orderly one or a kill. At the default
level only failures are written, and a request a person stopped is not one.

**Why.** These are the questions a bug report starts with: could it reach the service, was it
retried, did the server it needs start. Each is a fact about the program and none needs content.

`verified-by: bravebot_net::lib::a_failed_request_is_logged_as_host_status_and_kind_only`
`verified-by: bravebot_net::lib::a_stopped_request_is_not_a_failure_to_log`
`verified-by: bravebot_aichat::client::a_retry_is_written_to_the_diagnostic_log`
`verified-by: bravebot_bedrock::lib::a_retry_is_written_to_the_diagnostic_log`
`verified-by: bravebot_mcp::stdio::a_launch_that_failed_is_written_to_the_diagnostic_log`
`verified-by: bravebot_mcp::stdio::a_handshake_is_written_to_the_diagnostic_log`
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
`verified-by: bravebot_diag::lib::the_newest_log_is_the_greatest_timestamp_among_files_this_crate_made`

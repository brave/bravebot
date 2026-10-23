---
id: CRASH
title: Crash reports
status: normative
governs:
  - crates/backtrace/src/lib.rs
  - crates/backtrace/src/hook.rs
  - crates/backtrace/src/report.rs
  - crates/backtrace/src/send.rs
  - crates/backtrace/src/store.rs
  - crates/tui/src/crash_notice.rs
documented-by:
  - docs/website/docs/customize/configuration.md
  - docs/website/docs/security/security.md
---

## Scope

What this program tells its maintainers when it panics: the file left behind, what the file may
hold, when it is sent, where, and how a person stops it. The service is a Backtrace project that
Brave owns. A report is made for a person reading it afterwards, so it is neither
[trace.md](trace.md), which records gate decisions, nor [diagnostic-log.md](diagnostic-log.md),
which a person chooses to attach to an issue. It is sent without being asked for, which is why the
clauses below are about what it cannot say.

Only a Rust panic is reported. The executables covered are `bravebot`, `bravebot-host`,
`bravebot-rpc` and `bravebot-acp`.

## Clauses

<a id="CRASH-1"></a>
### CRASH-1: a report holds numbers, words fixed in the source, and one source location

A report says which build panicked (version, build stamp, which executable, how it was installed,
whether it is a debug build), the operating system and architecture, and the file, line and column
of the panic. It also holds the panic's message where that message is a `&'static str`, which the
standard library hands over only for a message with no run-time values in it. A message built from
values is left out and the report says there was none. There is no call stack, thread name,
environment variable, working directory, host name or account name.

A path is reported as it is when it is relative to the workspace or is a standard library path. A
path into a cargo registry is cut to the crate's own directory, any other absolute path to its last
three components, and a path with anything but visible ASCII characters, or longer than 240 bytes,
is written as `?`. A message is cut to 200 bytes.

**Why.** The process that panics holds prompts, file contents and credentials, and
[CRED-23](credential-protection.md#CRED-23) keeps a crash from leaving an image of them. A report
that carried the text of a failure would put what a prompt, a path or a server said into a service
nobody on the machine reviewed. Holding it by what each field can be, as
[DIAG-1](diagnostic-log.md#DIAG-1) does, makes it true of a panic written next year as well as of
the ones that exist.

`verified-by: bravebot_backtrace::report::a_report_holds_only_the_fields_it_lists`
`verified-by: bravebot_backtrace::report::a_message_built_at_run_time_is_not_in_the_report`
`verified-by: bravebot_backtrace::report::a_literal_message_is_kept_and_a_long_one_is_cut`
`verified-by: bravebot_backtrace::report::a_path_in_the_workspace_is_kept`
`verified-by: bravebot_backtrace::report::a_path_in_the_cargo_registry_is_cut_to_the_crate`
`verified-by: bravebot_backtrace::report::any_other_absolute_path_is_cut_to_its_last_three_components`
`verified-by: bravebot_backtrace::report::a_path_with_odd_bytes_or_too_long_is_written_as_a_question_mark`
`verified-by: bravebot_backtrace::privacy::a_report_holds_nothing_a_panic_was_built_from`

<a id="CRASH-2"></a>
### CRASH-2: a panic writes one file and sends nothing

The panic hook builds the report, writes it to `crash-reports/` in the state directory readable only
by the person, and returns to the hook that was installed before it, so the message on the screen
is the one that would have been there. It makes no request, reads no settings and starts no thread.
A file is never written over another.

**Why.** A panicking process may hold any lock, may be out of memory, and with `panic = "abort"`
ends as soon as the hook returns. A request made there can hang the process the person is watching
fail, or be cut off with the report lost. Writing the file takes one small write, and the launch
that sends it is a process that is working.

`verified-by: bravebot_backtrace::hook::a_panic_leaves_one_report_and_sends_nothing`
`verified-by: bravebot_backtrace::store::a_report_and_its_directory_are_readable_only_by_this_user`
`verified-by: bravebot_backtrace::store::a_report_is_never_written_over_another`

<a id="CRASH-3"></a>
### CRASH-3: reports go to one address, fixed when the build was made

The address is the Backtrace project's submission URL in `BACKTRACE_SUBMIT_URL`, read when the
build is made and baked in masked like the rest of the build's configuration. The environment at
run time and the settings files cannot name another. An address that is not `https` is not used. A
build made without it writes no report, installs no hook and sends nothing, which is every build
from source that did not have the variable set.

**Why.** Where a report goes is not for a checkout to say, and `.envrc` and settings files are what
a checkout carries. The token is in the path of the address, so a cleartext request would put it on
the wire.

`verified-by: bravebot_config::lib::a_baked_crash_report_address_is_used_only_when_it_is_https`
`verified-by: bravebot_config::lib::another_baked_value_is_not_a_crash_report_address`
`verified-by: bravebot_backtrace::lib::a_build_with_no_address_reports_nothing`
`verified-by: by-construction (the accessor reads the baked table and takes no environment or settings argument, so there is no input from either for it to read)`

<a id="CRASH-4"></a>
### CRASH-4: reports are on until a person turns them off

`BRAVEBOT_CRASH_REPORTS=0`, or `crashReports` set to `false` in the person's own settings file,
turns them off. Either one is enough, and neither turns them back on against the other. Only the
exact value `0` and a boolean `false` count, so a misspelling leaves reports on. Only the person's
own file is read: a settings file in a checkout can neither turn reports off for them nor turn them
on again after they turned them off, and the key is not reported as unread in either. With a switch
off no hook is installed, no report is written, no report is sent and no line is said.

**Why.** The default is on so that a released build tells its maintainers about the crashes people
have. The two switches are the ones the update check has ([UPDATE-11](updates.md#UPDATE-11)), so
they are where a person who turned that off will look. The checkout's file is left out for the
reason `language` leaves it out ([LOCALE-8](localization.md#LOCALE-8)): a layer that arrives with a
clone is not the person's statement about what their machine sends.

`verified-by: bravebot_backtrace::lib::either_switch_turns_reports_off`
`verified-by: bravebot_backtrace::lib::either_switch_stops_a_launch_that_would_report`
`verified-by: bravebot_backtrace::lib::the_settings_file_is_not_read_when_another_answer_has_already_said_no`
`verified-by: bravebot_backtrace::incognito::the_environment_switch_leaves_no_state_directory_and_sends_nothing`
`verified-by: bravebot_backtrace::incognito::the_persons_settings_file_stops_a_launch_and_a_string_does_not`
`verified-by: bravebot_config::settings::only_the_persons_own_file_says_whether_crash_reports_go`
`verified-by: bravebot_config::settings::only_a_boolean_turns_crash_reports_off`
`verified-by: bravebot_config::settings::the_crash_reports_key_is_not_reported_as_unread`

<a id="CRASH-5"></a>
### CRASH-5: a session with no state directory leaves no report and sends none

An incognito session, and a machine with no home directory, are given no state directory. They
install no hook, write no file, create no directory, send no request and say nothing about reports.
The directory is the one every other write to the state directory asks for, and it is asked for
inside the function every executable calls, so an executable passes no directory of its own. Each of
`bravebot`, `bravebot-host`, `bravebot-rpc` and `bravebot-acp` calls that function once with its own
word, and an executable the workspace builds that does not report says why.

**Why.** An incognito session adds nothing to the state directory ([incognito.md](incognito.md)),
and a report left there would be the trace the mode exists to avoid. Deciding the directory in one
place leaves an executable nothing to get wrong about it and no second opinion about incognito to
keep in step with the first.

`verified-by: bravebot_backtrace::lib::with_no_state_directory_nothing_starts`
`verified-by: bravebot_backtrace::incognito::an_incognito_launch_leaves_no_state_directory_and_sends_nothing`
`verified-by: bravebot_backtrace::incognito::an_ordinary_launch_leaves_one_report_and_sends_nothing_while_panicking`
`verified-by: bravebot_cli::crash_reports::every_reporting_executable_starts_it_once_with_its_own_word`
`verified-by: bravebot_cli::crash_reports::an_executable_that_does_not_report_names_nothing_of_the_crate`
`verified-by: bravebot_cli::crash_reports::every_executable_the_workspace_builds_says_whether_it_reports`
`verified-by: bravebot_cli::crash_reports::the_scan_sees_a_split_call_skips_comments_and_finds_every_function`
`verified-by: bravebot_cli::running::an_incognito_session_writes_no_log`

<a id="CRASH-6"></a>
### CRASH-6: a report is sent by a later launch, in the background, and few are kept

A launch that is reporting looks for waiting reports and sends them oldest first from a thread
nothing waits for. A one-shot command that ends first leaves them for the next launch. At most
five wait, the oldest being deleted as a sixth is written, and the one just written is never the
one deleted. A report older than two weeks is deleted unsent. Only a file named the way this
program names one is a report: anything else in the directory is neither sent nor deleted, and a
link is not followed. A file longer than 32 KiB is deleted unread. With nothing waiting no request
is made.

**Why.** Sending from the process that is starting costs the person nothing they can see, which
is [UPDATE-1](updates.md#UPDATE-1)'s argument for the update check. A crash on every launch would
otherwise fill the disk and send the same failure repeatedly, and a report of a build somebody has
replaced is told to the service after anyone could act on it.

`verified-by: bravebot_backtrace::store::only_a_name_this_crate_made_is_a_report`
`verified-by: bravebot_backtrace::store::at_most_five_reports_are_kept_and_the_oldest_go_first`
`verified-by: bravebot_backtrace::store::the_report_just_written_is_never_the_one_pruned`
`verified-by: bravebot_backtrace::store::waiting_reports_come_back_oldest_first`
`verified-by: bravebot_backtrace::store::a_report_older_than_two_weeks_is_dropped_unsent`
`verified-by: bravebot_backtrace::store::a_file_this_crate_did_not_make_is_left_alone`
`verified-by: bravebot_backtrace::store::a_link_with_a_reports_name_is_not_one`
`verified-by: bravebot_backtrace::send::reports_are_sent_oldest_first`
`verified-by: bravebot_backtrace::send::a_stale_report_is_dropped_without_a_request`
`verified-by: bravebot_backtrace::send::a_file_past_the_size_bound_is_removed_and_not_sent`
`verified-by: bravebot_backtrace::send::with_nothing_waiting_no_request_is_made`
`verified-by: bravebot_backtrace::launch::a_launch_sends_what_an_earlier_one_left_and_leaves_its_own_for_the_next`
`verified-by: by-construction (the sending thread's handle is dropped where it is spawned, so nothing can wait on it)`

<a id="CRASH-7"></a>
### CRASH-7: only the service's answer removes a report, and the reply is not read

A report is deleted when the service answers with a 2xx, and when it answers with any status other
than one that says to try later. A redirect is such a status, since redirects are not followed. A
connection that fails, a server that closes without answering and a status that says to try later
(408, 429, 500, 502, 503, 504) keep the report for a later launch, and the reports behind it are
not tried in the same launch. The body of the reply is never read.

**Why.** An offline machine, a captive portal and a firewall look the same from here as a service
that is down, and what is kept is already bounded by CRASH-6, so only an answer is a reason to
discard. What the reply's status decides is whether this program's own file is deleted. It reaches
no report, no request field, no turn and no screen, and nothing reads the body, so there is
nothing of the reply to declassify. This is the position of the update check ([UPDATE-5](updates.md#UPDATE-5)):
bytes that arrive over the network and decide only a local effect that no planner sees.

`verified-by: bravebot_backtrace::send::an_accepted_report_is_sent_as_written_and_removed`
`verified-by: bravebot_backtrace::send::a_refused_report_is_removed_and_not_sent_again`
`verified-by: bravebot_backtrace::send::a_server_that_is_unwell_keeps_every_report_and_is_asked_once`
`verified-by: bravebot_backtrace::send::a_service_that_cannot_be_reached_keeps_the_report`
`verified-by: bravebot_backtrace::send::a_server_that_closes_without_answering_keeps_the_report`
`verified-by: bravebot_backtrace::send::what_the_reply_says_in_its_body_decides_nothing`
`verified-by: by-construction (the response is dropped without a witness being minted for its body, so nothing in it can be read, and labels.md pins the files that mint one)`

<a id="CRASH-8"></a>
### CRASH-8: the request goes through the one egress call, as the file was written

The request is a `POST` of the file's bytes, unparsed, through the call every request here goes
through ([NET-1](network-egress.md#NET-1)). It carries a content type and a user agent and no
credential header. A redirect is answered with its status and followed nowhere. The crate holds no
HTTP client of its own, and Backtrace's own Rust crate is not used because it brings one.

**Why.** The address holds the token, and a redirect is a place the reply named, so following one
would send the token and the report where a server chose. Reading the file as bytes means a file
somebody else edited is sent as it is, to the person who owns the project, and decides nothing here.

`verified-by: bravebot_backtrace::send::an_accepted_report_is_sent_as_written_and_removed`
`verified-by: bravebot_backtrace::send::a_redirect_is_not_followed`
`verified-by: by-construction (the manifest depends on bravebot-net and on no crate that speaks to a network, and the layering test holds the manifest to this crate's row in layering.md)`

<a id="CRASH-9"></a>
### CRASH-9: the terminal says once that reports are sent

The first launch of the terminal interface that is reporting says so on its startup screen: what a
report holds, that it goes to Brave, and the two ways to turn it off. No launch after it says it
again, and a launch that is not reporting says nothing. The line is a catalog message with no
argument.

**Why.** Reports are on by default, so the person is told when the first one could be written, not
after one is sent. A launch that is not reporting has nothing to disclose, and a line about it
would read as though something was happening.

`verified-by: bravebot_tui::crash_notice::the_first_launch_that_reports_says_so_once_and_a_launch_that_does_not_says_nothing`
`verified-by: bravebot_backtrace::store::the_notice_is_recorded_once_and_is_not_a_report`

<a id="CRASH-10"></a>
### CRASH-10: nothing identifies an installation across reports

Each report carries a fresh random identifier, used both as its `uuid` and as its `guid` attribute.
No identifier, counter or timestamp of an earlier report, and nothing derived from the machine or
the account, is kept or sent.

**Why.** Counting crashes does not need to know which of them came from one installation, and a
value that persisted would let the service join one person's crashes to each other. The cost is
that the project's count of distinct `guid`s is a count of reports.

`verified-by: bravebot_backtrace::report::two_reports_from_one_machine_share_no_identifier`
`verified-by: bravebot_backtrace::report::the_identifier_is_a_version_four_uuid`
`verified-by: by-construction (the state directory holds the pending reports and one file recording that the notice was said, and no code writes an identifier to it)`

## Known costs

- **A panic is all that is reported.** A fault the process dies of without one, a bad pointer in
  a library, a stack overflow, running out of memory or being killed, leaves nothing. A minidump
  would cover them and holds the memory of a process that holds credentials, which
  [credential-protection.md](credential-protection.md#CRED-23) keeps out of crash artifacts.

- **The stack is one frame.** Release binaries are stripped, so the names a stack walk would find
  are the header symbol and nothing else, and the walk is work a failing process should not start.
  The panic's file and line are data in the binary and survive stripping, so they group reports.
  A fuller stack needs a dependency that reads frames (`backtrace`, not in the lock today) and
  symbol files uploaded at release time.

- **A message built at run time is lost.** `expect("...")` and `unwrap()` on an `Err` arrive as
  built strings, so the file and line are what say what failed.

- **No test runs an executable with an address baked in.** A test build has none, so the shared
  start function is run in child processes with an address a test states, and the executables are
  held to calling it by reading their sources. What an executable does with a baked address once
  it has called the function is what the child processes show.

- **The desktop application and the editor integration send without saying so.** `bravebot-host`,
  `bravebot-rpc` and `bravebot-acp` have no startup screen, and the one-shot and `--plain` modes
  print nothing there either. The README and the documentation say it, and the terminal says it
  once, but a person who only uses the desktop application reads it nowhere in the product.

- **The service sees the sender's network address.** Nothing in a report holds it, and what the
  service keeps of the connection is the service's.

- **A checkout's settings and a machine's managed settings cannot turn it off.** The environment
  variable is the one switch an administrator has.

- **A debug build with the address baked in reports as well.** A developer's shell that exports
  `BACKTRACE_SUBMIT_URL` bakes it into every build made there. The `debug` attribute is set so
  the project can leave those out.

- **The submission token is not removed from programs the agent starts.** In a shell that exports
  it, which is how a developer builds with it, a program the planner runs inherits it
  ([RUN-12](tools/run.md#RUN-12) lists the two credentials that are withheld). The token can add
  reports to one project and read nothing.

- **A report is retried until it is five deep or two weeks old.** Two launches that find the same
  report may both send it, and the service drops the second by its `uuid`.

- **A path may name the machine that built the binary.** A workspace path is relative and a
  dependency's is cut, but a generated file under an absolute build directory keeps its last three
  components.

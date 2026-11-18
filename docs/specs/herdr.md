---
id: HERDR
title: Reporting to herdr
status: normative
governs:
  - crates/herdr/src/lib.rs
  - crates/herdr/src/transport.rs
  - crates/tui/src/herdr.rs
  - crates/tui/src/app/terminal.rs
documented-by: docs/website/docs/using/herdr.md
---

## Scope

herdr is a terminal runtime that runs agents in panes and shows each pane's state. An agent it
cannot recognize from the screen can tell it instead, over a Unix socket herdr names in the pane's
environment: what the agent is doing (idle, working or blocked), and the command that resumes its
session when herdr restores the pane. This spec is the reporter a session uses for that: when it
is active, what a report carries, and what reporting may never do to the session, and the states
the terminal interface reports through it ([HERDR-8](#HERDR-8)), and the command it hands herdr
to reopen the session ([HERDR-9](#HERDR-9)).

The socket is local and names a process the person started, so a report is not network egress
([network-egress.md](network-egress.md)). It is still a channel into something that can type into
every pane herdr runs, which is why who holds the connection matters
([sandboxing.md](sandboxing.md)).

## Clauses

<a id="HERDR-1"></a>
### HERDR-1: the reporter is active only inside a herdr pane

The reporter is active when `HERDR_ENV` is `1` and `HERDR_PANE_ID` and `HERDR_SOCKET_PATH` are both
set, non-empty and valid UTF-8. Otherwise every call does nothing and nothing is connected to. It
reads no other variable.

**Why.** A socket path in the environment of a process herdr did not start could name anything's
socket, and a session must not write to one on the strength of a stray variable.

**Known cost.** herdr's socket is a Unix socket, so on Windows the reporter is never active and a
session there is not shown by herdr.

`verified-by: bravebot_herdr::tests::a_reporter_outside_herdr_is_inactive_and_connects_to_nothing`
`verified-by: bravebot_herdr::tests::a_herdr_variable_that_is_not_utf8_leaves_the_reporter_inactive`
`verified-by: bravebot_herdr::tests::a_reporter_asks_the_environment_for_the_three_herdr_variables_alone`

<a id="HERDR-2"></a>
### HERDR-2: a report carries the pane, bravebot's name, the state and what its caller hands it

A state report is a `pane.report_agent` request naming the pane from `HERDR_PANE_ID`, `bravebot` as
both source and agent, the state, a sequence number and, when the caller gives them, a message and
a resume command with its session id. A release is a `pane.release_agent` request with the pane,
the names and a sequence number. A request carries no other key, and nothing from the environment
other than the pane id.

**Why.** herdr shows the pane by what the report names, so a report under another name or pane
marks the wrong one; and what leaves the session is then only what an interface chose to say.

`verified-by: bravebot_herdr::tests::a_state_report_names_bravebot_the_pane_and_the_state`
`verified-by: bravebot_herdr::tests::what_a_report_carries_is_a_closed_set_of_keys`

<a id="HERDR-3"></a>
### HERDR-3: reporting never holds the session up

A report returns at once and is sent from a thread of the reporter's own, one request at a time,
each bounded to a second. A failure to connect, write or read is dropped. A report made while
another is being sent replaces any report still waiting, so only the newest is sent next, and a
report the same as the last one is not sent again.

**Why.** A herdr that is slow, stopped or gone must cost the session its reports, not its turns or
its keystrokes; and herdr needs where the session is now, which a queue of older states delays.

`verified-by: bravebot_herdr::tests::a_herdr_that_never_answers_holds_up_neither_reports_nor_release`
`verified-by: bravebot_herdr::tests::a_herdr_socket_that_is_not_there_is_ignored`
`verified-by: bravebot_herdr::tests::only_the_newest_waiting_report_is_sent`
`verified-by: bravebot_herdr::tests::an_unchanged_report_is_not_sent_again`
`verified-by: bravebot_herdr::transport::an_exchange_with_a_herdr_that_never_answers_ends_at_its_bound`

<a id="HERDR-4"></a>
### HERDR-4: sequence numbers rise, from the time

Each request's sequence number is above every one the process sent before and at least the time in
milliseconds since the Unix epoch.

**Why.** herdr ignores a report numbered at or below the last it took from that source in the pane,
so a session started again in the same pane, counting from one, would go unseen.

**Known cost.** A clock set back, or a session sending more than one report a millisecond and then
restarting within that lead, sends numbers herdr ignores until the clock catches up.

`verified-by: bravebot_herdr::tests::sequence_numbers_rise_and_start_from_the_time`

<a id="HERDR-5"></a>
### HERDR-5: a resume command herdr would refuse is not sent, and one is sent only with a state

A resume command is taken only when herdr would take it: a session id that is not empty, is at most
512 bytes and has no control character, and a command of one to 64 arguments, at most 8192 bytes
together, with no control character or apostrophe, whose first argument is a bare program name of
ASCII letters, digits, `_`, `-` and `.` not starting with `-`. One that is taken goes with the next
state report, and with the current state again when it changes; before the first state report
nothing is sent for it.

**Why.** herdr types the command into a shell to restore the pane, refuses a whole report whose
command it would not type, and takes a command only from the agent it already knows holds the
pane, which a state report in the same request tells it first.

`verified-by: bravebot_herdr::tests::a_resume_command_herdr_would_refuse_is_refused_here`
`verified-by: bravebot_herdr::tests::a_resume_command_goes_with_a_state_report_and_not_before_one`

<a id="HERDR-6"></a>
### HERDR-6: no program a session starts holds the connection to herdr

The socket for each request is created and marked close-on-exec under the gate every start of a
program takes, and connected outside it without blocking, so a herdr that has stopped accepting
fails the connect rather than holding the programs waiting at the gate.

**Why.** herdr's socket can type into every pane it runs. A confined program refused the socket's
path ([sandboxing.md](sandboxing.md)) that inherited a connection would reach it anyway.

`verified-by: bravebot_herdr::transport::the_connection_to_herdr_is_closed_on_exec`
`verified-by: bravebot_herdr::transport::a_connection_to_herdr_waits_for_a_program_being_started`

<a id="HERDR-7"></a>
### HERDR-7: nothing is sent after release

A release sends `pane.release_agent` and waits for herdr's answer for at most a quarter of a second.
After it, reports, resume commands and further releases send nothing. A reporter that never sent a
state releases nothing.

**Why.** A report reaching herdr after the session left would mark the pane as a session no longer
in it, and a session on its way out may wait only briefly.

`verified-by: bravebot_herdr::tests::nothing_is_sent_after_release`
`verified-by: bravebot_herdr::tests::a_session_that_never_reported_releases_nothing`

<a id="HERDR-8"></a>
### HERDR-8: the terminal interface tells herdr what its title says it is doing

Each state the title tracks ([PANEL-1](info-panel.md#PANEL-1)) is reported from the call that marks
the title, whether or not a title is written: ready as `idle`, a turn or command running as
`working`, and an approval, question or confirmation open as `blocked` with the fixed sentence
`herdr-waiting` from the catalog as its message. `terminalTitle = false` and an incognito session
turn the title off and leave the reports on. The interface reports `idle` once it has read the
settings, before the first question it can ask. A background session has no interface, and
`bravebot attach` relays its text without one, so neither reports.

**Why.** herdr notifies, rolls up and ends an `agent wait` on what it is told, so a state reported
from anywhere else than the title's could show a question as working or a turn as finished. A
report names no session, prompt or path, which is why turning the title off for what it names is
no reason to hide the session from herdr.

**Known cost.** A question asked before the first frame that does not mark the title, as the
startup trust prompt does not, shows in herdr as idle, as the title shows it as ready. An incognito
session is still shown by herdr as bravebot running in its pane, which herdr knows for as long as
the pane does; nothing of it is written under `~/.bravebot` ([INCOG-3](incognito.md#INCOG-3)).

`verified-by: bravebot_tui::herdr::each_state_the_title_tracks_reaches_herdr_as_its_own`
`verified-by: bravebot_tui::herdr::only_a_question_carries_a_message_and_it_is_the_fixed_one`
`verified-by: bravebot_tui::herdr::a_title_turned_off_still_tells_herdr`
`verified-by: bravebot_tui::app::tests::herdr::adopting_the_settings_tells_herdr_the_session_holds_its_pane_idle`

<a id="HERDR-9"></a>
### HERDR-9: the terminal interface tells herdr how to reopen the session, once its record is written

Once the session's record is on disk, the interface hands the reporter the session's id and the
command `bravebot --resume <id>`, with the modes [HERDR-10](#HERDR-10) carries after it, and herdr
is told them with the next state report ([HERDR-5](#HERDR-5)). The record is written when the
session's first prompt is sent, so a first turn that runs for hours has an id to hand over
([PANEL-9](info-panel.md#PANEL-9)). The program is the plain name
`bravebot`, as herdr types it into the pane's shell. The interface hands them over again on every
pass of its loop, so a session that becomes another, as `/clear` and `/branch` make it, tells herdr
the new id once the new record is written; an unchanged command is not sent again. An id that
begins with `-` is not sent, since `--resume` would read it as a flag. A background session has no
interface, and `bravebot attach` relays one without being it, so neither sends a command: the
session `attach` shows is reopened by attaching to it, which herdr cannot be told to do.

**Why.** herdr reopens what it was told when it restores the pane, and a record not yet written is
one `--resume` could not find. A command told once, for the first id the process knew, would
reopen the session before the one in front of the person.

**Known cost.** herdr has no way to be told a command no longer holds short of the session leaving
the pane. A session that became another keeps the previous one's command in herdr until the new
record is written, and a rewind that removed the record leaves herdr a command that finds no
session.

`verified-by: bravebot_tui::herdr::a_plain_session_is_reopened_by_bravebot_resume_and_its_id`
`verified-by: bravebot_tui::herdr::a_session_that_becomes_another_sends_the_new_id`
`verified-by: bravebot_tui::herdr::an_id_read_as_a_flag_and_a_session_not_yet_written_send_nothing`
`verified-by: bravebot_tui::app::tests::herdr::each_pass_of_the_loop_hands_herdr_the_id_once_the_record_is_written`

<a id="HERDR-10"></a>
### HERDR-10: the command carries what narrows the session and nothing that loosens it

A session run under `--locked` is reopened with `--locked`, and one run under `--safe` alone with
`--safe`. A `--sandbox` mode is carried where the mode the session would get without the flag is
looser than it and it is looser in nothing, by the one comparison a managed pin uses
([SANDBOX-22](sandboxing.md#SANDBOX-22)): the modes are not a line, so `strict` is not carried over
a `read-only` setting, since it writes what `read-only` does not. `--run-network closed` is carried
where the network would be open without it; `open` never is. Each path `--sandbox-deny-read` and
`--sandbox-deny-write` named is carried, since a denial only takes reach away; the denials a
settings file or the managed file wrote are not, since that file says them again. Nothing else the
command line said is carried. `--agent`, `--issue` and `--pr` are kept by the record and come back
with it. The system prompt flags hold for one run ([CLI-19](cli.md#CLI-19)), and their words would
be kept in herdr's own state. `--dangerously-skip-permissions`,
`--dangerously-bypass-approvals-and-sandbox`, `--accept-edits`, `--auto`, `--vet`, a looser
`--sandbox`, `--sandbox-allow-read`, `--sandbox-allow-write` and `--add-dir` each let the session
reach or ask less, and are not carried.

**Why.** herdr types the command with nobody asking for it. A mode that narrows the session must
survive that, since the record does not keep it and a session reopened without it would do what it
was started to refuse; a mode that loosens must not, since nobody typed it for the run herdr starts.

**Known cost.** `--settings` and `--log-level` are not carried, so a session that ran under a named
settings file is reopened under the layers alone; `--log-level` changes nothing a session may do. A
`--sandbox` mode that is narrower than the setting in one way and looser in the other, as `strict`
is beside `read-only`, is not carried either, so the session is reopened under the setting. A
denied path that holds an apostrophe or a control character makes a command herdr would refuse, so
no command is sent for that session ([HERDR-5](#HERDR-5)).

`verified-by: bravebot_tui::herdr::a_narrowing_sandbox_is_carried_and_a_loosening_one_is_not`
`verified-by: bravebot_tui::herdr::a_closed_network_is_carried_and_an_open_one_is_not`
`verified-by: bravebot_tui::herdr::a_path_the_command_line_denied_is_carried`
`verified-by: bravebot_tui::herdr::a_session_run_safe_or_locked_is_reopened_so`

<a id="HERDR-11"></a>
### HERDR-11: an incognito session tells herdr no command

An incognito session hands the reporter no id and no command, including one that resumed a record.
It still reports its state ([HERDR-8](#HERDR-8)).

**Why.** The command would reopen the session as an ordinary one that writes, and herdr keeps its
id in its own state, which outlives the session ([INCOG-3](incognito.md#INCOG-3)).

`verified-by: bravebot_tui::herdr::an_incognito_session_sends_no_resume_command`
<a id="HERDR-12"></a>
### HERDR-12: the terminal interface releases its pane when it leaves for good

When the interface ends, however it ended, it hands the terminal back and then releases
([HERDR-7](#HERDR-7)), whether or not the terminal could be handed back: a quit by `/exit`, Ctrl-C
or Ctrl-D, a failure that ends the interface, a session sent to the background and a session
reopened by starting `bravebot` again all release. A process started on the way out reports for
itself: a reopened session reports again from the new process, and a background session and
`bravebot attach` do not report ([HERDR-8](#HERDR-8)). Handing the terminal to an editor, a viewer
or the shell for a stop does not release, and nor does `/resume`, which changes the session without
leaving the interface. A herdr that never answers holds the way out up for at most the quarter of a
second a release may wait.

**Why.** Until it is released, herdr shows the pane as bravebot in its last state, with the command
that resumes its session, after the person is back at their shell. Releasing on a handover would
show a pane still running bravebot as a plain terminal while the person is in their editor, and
leave the session's next report refused as one from an agent that left.

**Known cost.** A panic releases nothing, since a panic makes no request
([CRASH-2](crash-reports.md#CRASH-2)), and nor does a session killed by a signal. herdr clears the
pane when it sees the shell prompt again, later than a release would.

`verified-by: bravebot_tui::app::tests::herdr::the_end_of_a_run_tells_herdr_the_run_has_left_its_pane`
`verified-by: bravebot_tui::app::tests::herdr::leaving_for_good_tells_herdr_the_run_has_left_its_pane_however_the_way_out_went`
`verified-by: bravebot_tui::app::tests::herdr::handing_the_terminal_over_for_a_while_leaves_the_run_in_its_pane`
`verified-by: bravebot_tui::app::tests::herdr::nothing_the_title_marks_after_leaving_reaches_herdr`
`verified-by: bravebot_tui::app::tests::herdr::a_herdr_that_never_answers_holds_the_way_out_up_only_briefly`

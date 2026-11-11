---
id: HERDR
title: Reporting to herdr
status: normative
governs:
  - crates/herdr/src/lib.rs
  - crates/herdr/src/transport.rs
  - crates/tui/src/herdr.rs
documented-by: none (gap: a page saying a session inside herdr reports to it, owed with the interface reporting its state)
---

## Scope

herdr is a terminal runtime that runs agents in panes and shows each pane's state. An agent it
cannot recognize from the screen can tell it instead, over a Unix socket herdr names in the pane's
environment: what the agent is doing (idle, working or blocked), and the command that resumes its
session when herdr restores the pane. This spec is the reporter a session uses for that: when it
is active, what a report carries, and what reporting may never do to the session, and the states
the terminal interface reports through it ([HERDR-8](#HERDR-8)).

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

---
id: SCHEDULE
title: Scheduled runs: pause, stop at a question, overlap
status: normative
governs:
  - crates/schedule/src/firing.rs
documented-by:
  - docs/website/docs/reference/cli.md
  - docs/website/docs/using/headless.md
---

## Scope

[CLI-42](cli.md#CLI-42) describes `bravebot schedule add|list|remove|run-now|history`: an entry, the
crontab line that starts it and the one-shot run that line starts. This spec covers what a stored
entry does when nobody is there: pausing it, what a firing does when the planner asks a question,
what happens when the previous firing has not ended, which expressions and flags an entry may
hold, and where the result of a run goes.

There is no `/schedule` command in a session. A session's own repeating prompt is `/loop`
([LOOP-12](loop.md#LOOP-12)), which lasts as long as the session, and a stored entry is made and
changed only at the command line by a person at a terminal. A command that could write an entry from
inside a session would be reachable by a turn's own request, and what an entry may do is decided
when it is written (see [cli.md](cli.md)). An in-session command is tracked as brave/bravebot#3681. The set of
commands in [commands.md](commands.md) is unchanged.

## Clauses

<a id="SCHEDULE-1"></a>
### SCHEDULE-1: `pause` and `resume` mark an entry without touching its crontab line

`schedule pause <id>` marks the entry paused and `schedule resume <id>` takes the mark off. Both
leave the crontab line and the entry as they are. A firing of a paused entry started by `run-now`
exits with the status for a refusal, says that the entry is paused, runs nothing and records nothing
in the history. `list` ends each row with `paused` or `active`. Pausing an entry that is paused,
resuming one that is not, and an id that names no entry are refused with the status for an argument.
`remove` deletes the mark with the entry.

**Why.** A person who wants a job to stop for a while should not have to retype its prompt and mode
to bring it back, and editing the crontab is the one thing `schedule` otherwise keeps them from
doing by hand.

`verified-by: bravebot_cli::scheduled_runs::a_paused_entry_keeps_its_line_and_its_firing_is_skipped_until_it_is_resumed`
`verified-by: bravebot_schedule::firing::a_pause_mark_is_put_on_and_taken_off`
`verified-by: bravebot_schedule::firing::forgetting_an_entry_deletes_what_was_kept_beside_it`

<a id="SCHEDULE-2"></a>
### SCHEDULE-2: a run started for an entry stops at the first question it is asked

`run-now` starts the run with the entry's id in the environment variable `BRAVEBOT_SCHEDULE_ENTRY`.
A run that finds the variable naming a stored entry ends its turn at the first question the planner
asks, keeps that question, answers nothing and exits with the status for a refusal. A later question
in the same turn does not replace the first. A value that names no stored entry changes nothing.

The question is carried to a file beside the entry and written as the lines a terminal shows, each
cleaned of what a terminal would act on. Nothing in the driver reads those lines to decide
anything.

**Why.** Declining the question and carrying on would let the planner act on the lack of an answer.
A person reading the question later is told what the run needed.

`verified-by: bravebot_cli::main::a_scheduled_run_stops_at_the_first_question_and_keeps_it`
`verified-by: bravebot_cli::main::a_run_stopped_at_a_scheduled_question_ends_refused`
`verified-by: bravebot_schedule::firing::a_question_is_kept_replaced_read_cleaned_and_cleared`
`verified-by: bravebot_schedule::firing::an_entry_named_in_the_variable_must_be_stored`

<a id="SCHEDULE-3"></a>
### SCHEDULE-3: `pending` shows the kept question and `clear` deletes it without reading it

`schedule pending [<id>]` prints, for each entry that has one, the question its last firing stopped
at, and says so when none has. `schedule clear <id>` deletes the kept question and leaves the entry.
It takes no text and no answer: the prompt is fixed when the entry is written, so a person who wants
the run to continue with an answer removes the entry and adds it again with a prompt that holds it.
A `run-now` of the entry deletes a question an earlier firing kept. An id that names no entry is
refused with the status for an argument.

`verified-by: bravebot_cli::scheduled_runs::a_stored_question_is_shown_by_pending_and_deleted_by_clear`
`verified-by: bravebot_cli::scheduled_runs::no_command_but_add_accepts_a_write_flag`

<a id="SCHEDULE-4"></a>
### SCHEDULE-4: a firing is skipped while the previous firing of the same entry still runs

`run-now` takes an exclusive lock on a file beside the entry before it starts the run, and holds it
until the run ends. A firing that cannot take the lock says that the previous one is still running,
starts nothing, records nothing and exits successfully. The lock belongs to the process, so a run
that was killed leaves none behind. Another entry is not held.

**Why.** A job that takes longer than its interval would otherwise pile up runs that write to the
same directory.

`verified-by: bravebot_cli::scheduled_runs::a_firing_is_skipped_while_the_previous_one_of_the_entry_still_runs`
`verified-by: bravebot_schedule::firing::a_second_hold_on_one_entry_is_refused_until_the_first_is_dropped`

<a id="SCHEDULE-5"></a>
### SCHEDULE-5: an expression is refused unless every number is inside its field's range

Each of the five fields takes `*`, a number or a range `a-b`, in a comma-separated list, each with
an optional `/step`. The minute is 0 to 59, the hour 0 to 23, the day of the month 1 to 31, the month
1 to 12 and the day of the week 0 to 7. A range runs upward, a step is a number greater than zero,
and a number is one to four digits. Anything else is refused with the status for an argument and
nothing is written.

**Why.** A value outside a range is one the scheduler rejects or reads its own way after the entry
was stored, and by then the person is not at the terminal.

`verified-by: bravebot_schedule::schedule::a_cron_field_outside_its_range_or_with_a_zero_step_is_refused`
`verified-by: bravebot_cli::scheduled_runs::a_cron_field_outside_its_range_writes_nothing`

<a id="SCHEDULE-6"></a>
### SCHEDULE-6: a stored run carries one of three modes and no other flag

The run an entry starts carries `--accept-edits`, `--dangerously-skip-permissions` or neither,
chosen on the `add` line. It never carries `--approve-input-lines`, `--auto` or the flag that also
switches off the sandbox, and `add` refuses them.

**Why.** An approval a program gives by sending a line is for a program that is there. A scheduled
run has none, so a stored entry that held the flag would approve whatever a later firing's input
said.

**Known cost: a layer that forbids the bypass mode after an entry was stored.** `add` refuses the
bypass flag where a `permissions.bypassUnreachable` layer is in force ([MODE-5](permission-modes.md#MODE-5)),
because `main` refuses the flag for every command before it dispatches. An entry stored earlier, or
edited by hand, still holds the mode. Its firing starts a run that carries the flag, and that run is
refused the same way with the status for an argument, which `history` shows. No separate check of
the entry is made at a firing.

`verified-by: bravebot_cli::scheduled_runs::a_layer_that_made_the_bypass_unreachable_refuses_it_at_add_and_at_a_firing`
`verified-by: bravebot_schedule::schedule::a_stored_run_carries_one_of_three_modes_and_never_an_approval_flag`
`verified-by: bravebot_schedule::schedule::a_typed_line_stores_no_approval_flag_whatever_mode_it_carries`
`verified-by: bravebot_cli::schedule::the_mode_is_read_only_unless_a_flag_was_typed`

<a id="SCHEDULE-7"></a>
### SCHEDULE-7: a run's result is handed to the program chosen at `add`, and to nothing else

`schedule add --deliver-to <path>` stores the absolute path of a program that exists and is a file,
with no control character in it, in the entry (`deliver=<path>`). Anything else is refused with the
status for an argument and nothing is written. `list` ends each row with `delivers to <path>` or
`delivers nowhere`, before `paused` or `active`. Only `add`, typed by a person at a terminal, sets
the destination: no run, result, pending question or later command changes it, and it is not one of
the words the job the scheduler holds is started with.

`run-now` records the run and drops the hold of SCHEDULE-4. Then, if the entry has a destination, it
starts that program once with the run's standard output on its standard input. The bytes handed
over are the `output.stdout` the run produced, moved on and not read again. The driver reads that
output once, for the session name `session_of` accepts. Neither that read nor anything
else in the driver decides whether, where or what to deliver from those bytes: whether to deliver
depends on the entry alone. A run that failed, was refused, or stopped at its first question
(SCHEDULE-2) hands over what it printed as any run does. A run stopped at a question prints its
`--json` result line: `ok` false, status 4, a refusal at the `scheduled_question` gate, and the
call to `ask_user` without its text. That line is the whole of what the destination receives. The
lines kept for `pending` are not handed over, so the question and its choices do not reach the
destination, and a later firing does not hand over an earlier firing's question.

The destination is started with no argument, with the environment of the firing less this agent's
own credentials ([HOOK-4](hooks.md#HOOK-4) says the same of a hook) and the search path reduced.
It is a process group of its own, started in the person's home and not in the entry's directory,
with its standard output and error discarded. It is given 60 seconds. A program still running then
is stopped together with the programs it started, as a hook is ([HOOK-7](hooks.md#HOOK-7)). The
hold is dropped before the destination starts, so a destination
that is slow or never ends does not make the next firing look like one still running.

A destination that cannot be started, ends with a failure status or is stopped at its limit is
reported by one sentence on standard error that names the entry and the program path and carries
nothing the run or the program printed. It changes neither the line `run-now` prints, nor the
recorded run, nor the entry, nor the status `run-now` exits with. Delivery is not recorded in the
history. A paused entry (SCHEDULE-1) and a firing skipped as still running (SCHEDULE-4) start no
run and deliver nothing. A trigger firing ([triggers.md](triggers.md)) is not a scheduled run and
delivers nothing.

All three schedulers start `bravebot schedule run-now <id>`, so a cron, launchd or systemd firing
reaches the same path.

**Why.** Results were kept and listed and sent nowhere, so a person who wanted one had to go and
read it. The destination is a program the person named at `add`, so what receives the result is
decided by them and not by anything a run printed. The bytes are carried to it without being read,
so the result decides nothing. A run that failed still prints what it printed, and a person with a
notifier wants to know it failed.

**Release.** A run's text may hold content that was private in the run, and a program receives the
bytes outside the policy ([ROUTE-3](routing.md#ROUTE-3)). The release is the person's. They named
the program for this entry in words they typed at a terminal, as they name a hook
([HOOK-3](hooks.md#HOOK-3)) or a connector authority ([CONNECTOR-2](connectors.md#CONNECTOR-2)), and
no run supplies or changes it. [LABEL-6](labels.md#LABEL-6) lists the standard input of a program a
person endorsed among the places bytes may move to without being inspected. The destination is that
program: it is chosen by the person at `add`, takes no argument, and receives the standard output of a separate run of bravebot,
which the driver of `run-now` moves to its standard input without reading it.
[ROUTE-3](routing.md#ROUTE-3) speaks of content carrying a label inside a session. The captured
output carries none in the process that moves it, and the run that printed it already wrote it to
its own standard output, which the scheduler puts on the person's log or mail.
[ROUTE-5](routing.md#ROUTE-5) is about the endorsement of an effect the planner proposes, bound to
one value and used once. `run-now` and a firing are not tool calls of a planner, so the endorsement
here is the person's standing choice of program at `add`. Nothing the run printed reaches a routing
field.

**Not a hook, and not a question.** A hook is told the moment and nothing else
([HOOK-5](hooks.md#HOOK-5)) and decides nothing ([HOOK-6](hooks.md#HOOK-6)), so it cannot carry the
result. `prompt-shown` fires only for a question that reaches a person, and a scheduled run reaches
nobody ([PROMPT-9](prompting.md#PROMPT-9), [HOOK-2](hooks.md#HOOK-2)), so it does not fire for the
question a run stops at either.

**Known costs.** The result goes only to the program named at `add`, and changing it means removing
the entry and adding it again. Nothing carries a reply back to a run: no inbound path that answers
a question the run stopped at is provided. Whether a destination received the result is known only
from standard error, which the crontab line discards (`>/dev/null 2>&1`) and launchd and systemd
keep in their own logs. A destination is not confined: it runs with the person's own authority, as
a hook does. The program is checked when `add` stores it and started on every later delivery, so a
scheduled run in accept-edits or bypass mode can overwrite it, and the replacement is started with
the person's authority at the next delivery. This is accepted.

`verified-by: bravebot_schedule::schedule::the_bytes_a_run_printed_reach_the_destination_unmodified`
`verified-by: bravebot_schedule::schedule::hostile_text_changes_neither_what_is_run_nor_what_is_recorded`
`verified-by: bravebot_schedule::schedule::a_destination_that_fails_changes_nothing_but_the_notice`
`verified-by: bravebot_schedule::schedule::a_run_that_failed_hands_over_what_it_printed_as_any_run_does`
`verified-by: bravebot_schedule::schedule::the_hold_is_let_go_of_while_the_destination_runs`
`verified-by: bravebot_schedule::schedule::a_destination_that_never_ends_is_stopped_with_what_it_started`
`verified-by: bravebot_schedule::schedule::the_destination_starts_outside_the_entry_directory`
`verified-by: bravebot_schedule::schedule::nothing_a_run_printed_reaches_the_entry_or_its_destination`
`verified-by: bravebot_schedule::schedule::a_destination_must_be_an_absolute_path_to_a_file_that_exists`
`verified-by: bravebot_schedule::schedule::add_refuses_a_destination_that_is_not_usable_with_the_argument_status_before_storing`
`verified-by: bravebot_schedule::schedule::list_shows_the_destination_before_the_paused_or_active_word`
`verified-by: bravebot_cli::scheduled_runs::a_run_is_handed_to_the_destination_chosen_at_add_whichever_scheduler_started_it`
`verified-by: bravebot_cli::scheduled_runs::a_skipped_firing_delivers_nothing_and_a_failing_destination_changes_nothing`
`verified-by: bravebot_cli::scheduled_runs::an_earlier_question_is_not_delivered_again_by_the_next_firing`
`verified-by: bravebot_cli::scheduled_runs::a_run_stopped_at_a_question_delivers_its_stdout_and_not_the_kept_question`
`verified-by: bravebot_cli::scheduled_runs::the_destination_is_started_without_the_agents_own_credentials`
`verified-by: bravebot_cli::scheduled_runs::a_trigger_firing_delivers_nothing_to_a_schedule_entrys_destination`

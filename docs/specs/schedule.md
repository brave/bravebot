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
what happens when the previous firing has not ended, and which expressions and flags an entry may
hold.

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

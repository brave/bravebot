---
id: LOOP
title: Repeating a prompt
status: normative
governs:
  - crates/tui/src/loops.rs
  - crates/tui/src/app.rs
  - crates/tui/src/state.rs
  - crates/tui/src/status.rs
  - crates/tui/src/render.rs
guards:
  - symbol: Session::start_loop
  - symbol: Session::watch_again
  - symbol: Running::dispatched
documented-by: docs/website/docs/reference/commands.md
---

## Scope

`/loop`: sending one prompt again and again until somebody stops it. What the repeated line is,
where the interval comes from, when a tick fires, and what ends a loop. A turn that asks to be asked
again starts one too ([LOOP-14](#LOOP-14)), and everything here holds for that loop as well.

Not what a tick then does, which is a turn like any other. The tool a turn uses to say when it
should be asked again is [tools/schedule-next.md](tools/schedule-next.md), and what the planner
is told about being inside a loop is a skill, in [skills.md](skills.md). That a `/` line is a
command at all, and that only a key press produces one, is [commands.md](commands.md).

## What repeats

<a id="LOOP-1"></a>
### LOOP-1: the line a loop repeats is the one the person typed, for as long as it runs

The argument to `/loop`, with any interval taken off it, is settled when Enter is pressed and is
sent unchanged on every tick. Nothing a turn reads, writes, says or returns can add to it, edit
it or replace it. A planner may say *when* the next tick happens and there is nowhere for it to
say what the next tick asks.

**Why.** A schedule that a turn could write the next prompt into is a turn that can rewrite its
own instructions, and the whole point of a loop is that it asks the same question again. The
person endorsed a line; every tick has to be that line.

`verified-by: bravebot_tui::app::the_loop_command_sends_what_is_left_after_the_interval`
`verified-by: bravebot_tui::state::the_first_tick_of_a_loop_goes_immediately`
`verified-by: bravebot_agent::turn::a_tick_of_a_self_paced_loop_says_when_to_run_again`

<a id="LOOP-2"></a>
### LOOP-2: a tick is a prompt, never a command

`/loop 5m /status` sends the seven characters `/status` to the planner every five minutes. It
does not run the status command, and no argument to `/loop` ever reaches the command dispatcher.

**Why.** A command is dispatched from a key press and from nothing else, and a timer is not a key
press. A loop that could run commands would be a way to make this program act on a schedule
nobody was watching, with each firing endorsed by a keystroke made once, long ago.

`verified-by: bravebot_tui::state::a_loop_whose_prompt_looks_like_a_command_still_sends_it_as_a_prompt`

<a id="LOOP-3"></a>
### LOOP-3: an interval is read off the front of the argument, or off an `every` clause at its end

In that order, and nowhere else.

| The argument | The interval | What is sent |
|---|---|---|
| `5m check the deploy` | 5 minutes | `check the deploy` |
| `check the deploy every 20m` | 20 minutes | `check the deploy` |
| `check the deploy every 20 minutes` | 20 minutes | `check the deploy` |
| `check every PR` | none, so each turn paces it | `check every PR` |
| `check everything 20m` | none, so each turn paces it | `check everything 20m` |
| `5m check the deploy every 20m` | 5 minutes | `check the deploy every 20m` |
| `5m`, or `every 5m` | there is nothing to send | nothing; the command says what it needs |

A leading token counts only when it is a number and one of `s`, `m`, `h` or `d` and nothing
else. A trailing clause counts only when `every` is a word of its own, with whitespace or the edge
of the line on each side of it, and a time expression is the whole of what follows it. That is what
keeps `check every PR` a sentence rather than a sentence with its last two words taken off, and a
word that only begins with those five letters, such as `everything`, is not the clause at all.

`verified-by: bravebot_tui::loops::an_interval_written_first_is_taken_off_the_front`
`verified-by: bravebot_tui::loops::every_unit_letter_is_understood_at_the_front`
`verified-by: bravebot_tui::loops::an_interval_written_last_is_taken_off_the_end`
`verified-by: bravebot_tui::loops::a_trailing_interval_may_spell_its_unit_out`
`verified-by: bravebot_tui::loops::every_without_a_time_after_it_is_words_rather_than_an_interval`
`verified-by: bravebot_tui::loops::a_leading_interval_wins_over_a_trailing_one`
`verified-by: bravebot_tui::loops::a_line_with_no_interval_is_paced_by_the_planner`
`verified-by: bravebot_tui::loops::a_word_that_is_not_a_time_stays_part_of_the_prompt`
`verified-by: bravebot_tui::loops::a_word_that_merely_starts_with_every_is_not_an_interval`
`verified-by: bravebot_tui::loops::a_count_too_large_to_be_a_duration_is_not_an_interval`
`verified-by: bravebot_tui::loops::an_interval_with_nothing_to_send_is_not_a_request`
`verified-by: bravebot_tui::app::an_interval_with_nothing_after_it_says_what_the_command_needs`

## When a tick fires

<a id="LOOP-4"></a>
### LOOP-4: the first tick of a loop somebody typed goes immediately

`/loop` sends its prompt at once rather than waiting out the first interval.

**Why.** Somebody who has just asked for something every five minutes wants to see it happen
once, while they are still watching, and decide whether it was the right thing to ask for. A loop
whose first sign of life is five minutes of nothing is one nobody can tell is running.

A loop a *turn* started is the other way round, for the same reason read the other way: that turn
has just taken the look it is reporting, so an immediate tick would send the line again before the
person had read the first answer and come back with the same look twice. See
[LOOP-14](#LOOP-14).

`verified-by: bravebot_tui::state::the_first_tick_of_a_loop_goes_immediately`
`verified-by: bravebot_tui::state::a_watch_a_turn_arranged_sends_nothing_until_the_wait_is_up`

<a id="LOOP-5"></a>
### LOOP-5: the gap is measured from the end of a tick, not from its start

`every 5m` means five minutes between runs.

**Why.** Measured from the start, a turn that outlasts its own interval would be due again the
instant it drew breath, and a loop over slow work would become a continuous one. The interval is
how often somebody wants to be told something, and telling them takes time too.

`verified-by: bravebot_tui::loops::a_tick_in_flight_is_not_due_again`
`verified-by: bravebot_tui::state::a_tick_that_says_when_to_wake_arms_the_next_one`

<a id="LOOP-6"></a>
### LOOP-6: a tick waits for an idle session, and never interrupts

A due tick is held while a turn is running and while anything the person queued is still waiting,
and goes when both are done. A prompt the person typed in the middle of a loop is not a tick of
it: it is not asked when the next tick is due, and its ending does not reset the clock.

**Why.** A schedule is a request to be asked again, not a licence to interrupt. The person is
still the one using this session.

`verified-by: bravebot_tui::state::a_tick_waits_for_the_turn_in_flight_and_for_what_is_queued`
`verified-by: bravebot_tui::state::a_prompt_typed_during_a_loop_is_not_a_tick_of_it`

<a id="LOOP-7"></a>
### LOOP-7: how fast a loop may go, and how slow

| The wait | Shortest | Longest |
|---|---|---|
| an interval the person gave | 5 seconds | 7 days |
| a delay a turn asked for | 1 second | 1 hour |

A number outside the bounds becomes the nearer of the two, and a person who wrote one is told
what it became rather than left believing they are watching something ten times more closely than
they are.

**Why neither floor paces a watch.** Both waits are measured from the end of a tick, and a tick is
a whole turn, so a turn that takes a minute spaces itself out whatever the floor says. Neither
number is what makes a loop fast. They are non-zero only because a loop with no gap at all is a way
to spend a rate limit rather than a way to watch something, and the person's is the higher of the
two because the interface has to stay usable between ticks, where a turn's own length already
supplies that gap.

**Why the ceilings differ.** An interval the person typed is their own number, and `10s` means ten
seconds, so it is capped only at the age a loop cannot outlive anyway. A delay a *turn* asked for is
the planner's number rather than the person's, so it is held much more tightly at that end: a person
started the loop and is entitled to see it do something, and a turn that wants longer than an hour
can say so in its answer, where somebody reads it, rather than by going quiet for a day.

`verified-by: bravebot_tui::loops::an_interval_the_person_gave_is_kept_down_to_the_floor`
`verified-by: bravebot_tui::loops::an_interval_faster_than_the_floor_is_raised_to_it_and_said_so`
`verified-by: bravebot_tui::loops::an_interval_longer_than_a_loop_may_live_is_capped`
`verified-by: bravebot_tui::loops::an_interval_within_the_bounds_is_reported_as_unadjusted`
`verified-by: bravebot_tui::loops::a_wait_a_turn_asked_for_is_held_to_the_bounds`
`verified-by: bravebot_tui::state::an_interval_outside_the_bounds_is_reported_as_the_one_that_will_happen`
`verified-by: bravebot_agent::tools::a_wait_outside_the_bounds_is_reported_as_the_one_that_will_happen`

<a id="LOOP-8"></a>
### LOOP-8: where the person gave an interval, the driver keeps it and no turn can change it

A turn that asks for a different wait is answered by the interval that was typed. Only a loop
nobody gave an interval for asks the planner anything about timing.

`verified-by: bravebot_tui::loops::a_paced_loop_ignores_what_a_turn_asked_for`
`verified-by: bravebot_tui::loops::a_self_paced_loop_waits_as_long_as_the_turn_asked`

<a id="LOOP-9"></a>
### LOOP-9: a self-paced tick that says nothing is woken once more, then the loop ends

Twenty minutes later, once. A second tick that ends without saying when to run again ends the
loop.

**Why.** The first silence is a turn that forgot. The second is a loop nobody is running, and
waking it every twenty minutes for the rest of the session helps nobody. A tick that does say
when to run again restores the fallback, because the budget is for turns that stopped answering.

`verified-by: bravebot_tui::loops::a_self_paced_turn_that_says_nothing_is_woken_once_more_and_then_the_loop_ends`
`verified-by: bravebot_tui::loops::a_turn_that_says_when_to_wake_restores_the_fallback`

<a id="LOOP-10"></a>
### LOOP-10: a tick is told that it is one, and which kind of loop it is in

The turn carries which tick it is and whether the loop is self-paced, and both are said in the
planner's own preamble. A tick paced by the person is told the timing is theirs and that there is
no tool for it; a self-paced tick is told the loop lasts as long as it keeps saying when to run
again.

**Why.** The driver is the only thing that knows a turn is a tick. A planner that cannot tell
answers as though somebody had just typed the line for the first time, which is the failure a loop
exists to avoid, and one that half-knows goes looking for the scheduling tool an interval loop
never has and tells the user it is missing. Both were observed before this clause existed.

`verified-by: bravebot_agent::turn::a_tick_is_told_that_it_is_one_and_which_kind_of_loop_it_is_in`

## What ends one

<a id="LOOP-11"></a>
### LOOP-11: five things end a loop, and each of them says so

| What | When |
|---|---|
| the person asks | `/loop stop`, which leaves the turn in flight running; typed during one it ends the loop when the queue reaches it |
| the person interrupts | Ctrl-C, read against the loop after the turn in flight and the line in the box, and before leaving |
| a turn is stopped | any turn cancelled while a loop runs, whether or not it was a tick |
| the session moves on | `/clear`, and leaving |
| age | seven days after it started |

Ctrl-C reaches the loop before it reaches the session, so the key that stops a thing that keeps
happening is not also the key that ends everything. It reaches the half-written line first,
because that is nearer still.

**Why the command is not the same ending as the key.** Ctrl-C is read against the turn in flight
before it is read against the loop, so ending a loop with it during a tick cancels work that was
half done. The command ends the repeating and nothing else, which is what somebody wants who has
seen enough of a loop but not of the turn it is in the middle of. It is also the only one of these
that can be typed, so it is the ending a person reaches for after the sentence announcing the loop
has scrolled away: the others are a key nobody named, a session ending, and a week.

**Why it may arrive a turn later.** A line typed during a turn waits in the queue, the way every
line typed during a turn waits, so an ending asked for mid-tick happens when the queue is reached
rather than on the press. Nothing is lost in the wait: a tick waits on an empty queue as well as on
an idle session, so the loop cannot send one more turn out ahead of its own ending.

`verified-by: bravebot_tui::app::the_loop_command_ends_the_loop_when_asked_to_stop`
`verified-by: bravebot_tui::app::asking_to_stop_a_loop_during_a_turn_ends_it_when_the_queue_is_reached`
`verified-by: bravebot_tui::state::a_tick_waits_for_the_turn_in_flight_and_for_what_is_queued`
`verified-by: bravebot_tui::app::asking_to_stop_a_loop_that_is_not_running_says_so`
`verified-by: bravebot_tui::app::interrupting_stops_the_loop_before_it_leaves`
`verified-by: bravebot_tui::app::interrupting_clears_the_line_before_it_stops_the_loop`
`verified-by: bravebot_tui::state::clearing_the_session_ends_the_loop`
`verified-by: bravebot_tui::state::stopping_a_loop_says_so_and_says_nothing_when_there_was_none`
`verified-by: bravebot_tui::loops::a_loop_older_than_a_week_has_aged_out`

<a id="LOOP-12"></a>
### LOOP-12: a loop is never written down

It is not in the session record, so it is not restored by a resume and does not survive the
process.

**Why.** A schedule that outlived the session that set it would start sending prompts at somebody
who opened a conversation to read it, with no visible cause and nothing in the transcript to
explain it. The gesture that starts a loop is the gesture that keeps it: while this session is
open.

`verified-by: by-construction (the loop is a private field of the interface's session state and is not among the fields written to a session record)`

<a id="LOOP-13"></a>
### LOOP-13: what is going to happen is on the screen

Each tick is announced with its number, and with how many ticks in a row have reported finding
nothing where there have been any. Three places say a loop is live, and a session with no loop says
nothing about loops in any of them:

| Where | What it says |
|---|---|
| the row under the input box, whichever mode it is in | that a loop is live, and when the next tick is due where a moment is known |
| `/loop`, with no argument | the repeated line, the pacing, when the next tick is due, and how to end it |
| `/status` | the repeated line, the pacing, and when the next tick is due |

The row counts down to the next tick while one is ahead, and says only that a loop is live where no
moment is known or the moment has passed. It is redrawn at least once a second for as long as a loop
is live, whether or not anything else has happened.

**Why the row under the box.** Everything else on the screen is a record of something that has
already happened. Between ticks a loop has nothing in the transcript but the sentence that
announced it, which has scrolled away by the time somebody wonders. Without this row a session
spending a turn every five minutes while nobody types reads exactly like an idle one, and the row is
the only part of the screen that is about now rather than about what has already happened.

**Why it is redrawn on its own.** Frames are drawn from what has changed, and a loop waiting for its
next tick changes nothing. A row drawn once therefore holds the moment it was drawn for the whole of
an interval: a five-minute loop says `next in 4m 58s` for five minutes and corrects itself only when
somebody presses a key for an unrelated reason. A countdown is the one thing on this screen that goes
stale by standing still, so it is the one thing that cannot wait for an event.

**Why a moment already gone is not spelled.** A tick whose moment has passed is one the session is
not free to take yet, because a turn is running or the queue is holding a line. A row counting down
to `next in 0s` and sitting there reads as a loop that has stalled rather than one waiting its turn.

**Why the count of quiet ticks.** It is the difference between a loop that is working and a loop
that has nothing to do, and without it a long watch is twenty identical answers nobody reads.

`verified-by: bravebot_tui::render::the_hint_line_says_a_loop_is_live`
`verified-by: bravebot_tui::render::the_hint_line_says_a_loop_is_live_in_shell_mode_too`
`verified-by: bravebot_tui::render::the_loop_part_names_a_moment_only_while_one_is_still_ahead`
`verified-by: bravebot_tui::app::a_countdown_is_owed_a_frame_once_a_second_and_only_while_a_loop_runs`
`verified-by: bravebot_tui::render::the_hint_line_says_nothing_about_a_loop_in_a_session_with_none`
`verified-by: bravebot_tui::state::the_loop_report_says_what_is_repeating_and_how_to_end_it`
`verified-by: bravebot_tui::state::a_session_with_no_loop_says_so_when_asked`
`verified-by: bravebot_tui::status::the_report_says_what_is_repeating_and_when_it_is_next_due`
`verified-by: bravebot_tui::status::a_session_with_no_loop_does_not_mention_one`
`verified-by: bravebot_tui::loops::quiet_ticks_are_counted_until_one_reports_something`

<a id="LOOP-14"></a>
### LOOP-14: a turn may start a loop over the person's line, and over no other

A turn that asks to be asked again while nothing is looping starts a loop
([SCHED-6](tools/schedule-next.md#SCHED-6)). It repeats the line that turn was running, is always
self-paced, and its first tick is the wait away rather than immediate. Three things it is not:

| Not | Because |
|---|---|
| a loop over a line the person did not write | the sentence this program writes to carry a goal on is not a line anybody endorsed, so a turn running one gets no loop |
| a way past a goal | a session does one thing at a time, and it says so rather than dropping the condition; a person's own `/loop` may replace a goal, since they are there to mean it |
| approved separately | the person asked to be told when something changed, and a prompt asking whether they meant it is a question they have already answered |

**Why.** A request to be told when something changes cannot be answered inside one turn: the turn
that reads a file now cannot see it written later. Before this clause the honest answer was one read
and a suggestion that the person start a loop themselves, and what shipped instead was one read
reported as a watch. Nothing about LOOP-1 is loosened: the line still belongs to whoever typed it,
and the turn chooses only when it is sent again.

**Why the wait is self-paced whatever the turn said.** An interval is a number a person gives, and
there is nowhere for a turn to give one. What it gives is the wait until the next look, and it is
asked again then, which is exactly what [LOOP-9](#LOOP-9) already bounds.

`verified-by: bravebot_tui::app::only_a_line_the_person_wrote_becomes_a_watch_the_turn_asked_for`
`verified-by: bravebot_tui::state::a_watch_a_turn_arranged_sends_nothing_until_the_wait_is_up`
`verified-by: bravebot_tui::state::a_watch_a_turn_arranged_does_not_replace_a_goal`
`verified-by: bravebot_tui::loops::a_loop_a_turn_asked_for_starts_a_wait_away_rather_than_now`
`verified-by: bravebot_tui::loops::a_loop_a_turn_asked_for_is_paced_by_the_turns`

## What the argument may be

<a id="LOOP-15"></a>
### LOOP-15: the command reads its argument three ways, and `stop` only as the whole line

| The argument | What the command does |
|---|---|
| a line, with or without an interval | starts a loop over it, per [LOOP-3](#LOOP-3) |
| nothing at all | says what is repeating and how to end it, per [LOOP-13](#LOOP-13) |
| `stop`, in any case, with or without an interval | ends the loop, and says there was none where there was not, per [LOOP-11](#LOOP-11) |

An interval with nothing after it is none of the three: it says what the command needs and starts
nothing, per [LOOP-3](#LOOP-3). The reserved word is the ending whenever it is the whole of the line
that would be sent, so `/loop STOP`, `/loop stop every 5m` and `/loop 5m stop` all end the loop,
while `/loop stop the deploy` is a loop over `stop the deploy` and `/loop stop the deploy every 5m`
is that line every five minutes.

**Why the bare word answers rather than refusing.** The two things a person needs from a command
about a loop are the two things the transcript cannot give them: what is repeating, and a way to
end it. Neither is a line to send, so the forms that do them send nothing, which is how `/goal` and
`/watch` already read.

**Why an interval on its own is not the bare word.** Somebody who typed `/loop 5m` and pressed
Enter is halfway through starting a loop, not asking about one, and answering with a report about
the loop they were replacing would be answering a question they did not ask.

**Why the reserved word is the whole line or nothing.** This is the one place in the command where a
word could be read as an instruction instead of as the line somebody wanted repeated. A first token
taken as the ending would send them the rest of their own sentence every five minutes, and they
would have typed both halves of it. An interval is not part of that line, so a pace beside the word
leaves it the whole of what would be sent: `stop every 5m` is somebody ending the loop that runs
every five minutes, and a loop sending the bare word `stop` to a model is nothing anybody means by
anything. The case is not read either, because a miss here is not a message: a line meant as the
ending and not taken as one becomes a loop sending `Stop` until somebody presses Ctrl-C twice.

`verified-by: bravebot_tui::loops::the_bare_command_asks_what_is_repeating`
`verified-by: bravebot_tui::loops::stop_on_its_own_ends_the_loop`
`verified-by: bravebot_tui::loops::the_word_ends_the_loop_whatever_its_case`
`verified-by: bravebot_tui::loops::an_interval_beside_the_word_still_ends_the_loop`
`verified-by: bravebot_tui::loops::a_line_that_begins_with_stop_is_still_a_line_to_repeat`
`verified-by: bravebot_tui::loops::an_interval_with_nothing_to_send_is_not_a_request`
`verified-by: bravebot_tui::app::the_bare_loop_command_says_what_is_repeating`
`verified-by: bravebot_tui::app::a_loop_over_a_line_beginning_with_stop_is_still_a_loop`
`verified-by: bravebot_tui::app::an_interval_with_nothing_after_it_says_what_the_command_needs`

## Known costs

- **A loop keeps spending.** Every tick is a turn, with the whole conversation re-sent, and
  nothing here bounds the total. The interval and the session's own life are the only limits, and
  a five-minute loop left open overnight is a hundred and fifty turns nobody read.
- **A self-paced loop is paced by the thing it is watching over.** The planner chooses the wait
  from a context it also wrote, so a turn that misjudges what it is waiting for will keep
  misjudging it. The ceiling holds a slow loop to one turn an hour; at the other end the floor
  holds almost nothing, and what a fast loop costs is set by how long a turn takes. Nothing holds
  the usefulness.

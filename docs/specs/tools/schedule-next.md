---
id: SCHED
title: schedule_next
status: normative
governs:
  - crates/agent/src/tools.rs
---

## Scope

Saying when this turn should be asked again. `delay_seconds` and `noop` are routing; `reason` is
content. The result is a confirmation naming the wait that will actually happen.

What a loop is, where its prompt comes from, and what ends one is [loop.md](../loop.md).

## Clauses

<a id="SCHED-1"></a>
### SCHED-1: the only thing this decides is a moment

There is no argument for what the next run asks. The prompt is the line the person typed, held by
the interface, and it is sent again unchanged.

**Why.** This is what makes the routing approvable on its own. "Ask me that again in twenty
minutes" can be read and agreed to without knowing what "that" is; a field naming the next prompt
would make the same call unreadable, and would let a turn write its own next instruction.

`verified-by: bravebot_agent::tools::nothing_on_this_tool_says_what_the_next_turn_asks`

<a id="SCHED-2"></a>
### SCHED-2: withdrawn, it is offered to a tick of a self-paced loop and to nothing else

Replaced by [SCHED-6](#SCHED-6), which offers it to every turn except a tick the person timed. The
rule it stated left a session asked to report a change with nothing to report it from: one turn
cannot both read a file now and see it change later, so a turn that could not arrange the later
look answered from a single read and left nothing watching. SCHED-1 is what the rule was protecting
and SCHED-1 is untouched: a turn may still choose only a moment, and never what the next one asks.

<a id="SCHED-3"></a>
### SCHED-3: the wait is held to its bounds before it is reported back

Between a second and an hour. The number the planner is told is the number it is getting, not the
number it asked for.

**Why.** A tool that echoed what it was given would have the next answer describing a schedule
that is not going to happen.

`verified-by: bravebot_agent::tools::a_wait_outside_the_bounds_is_reported_as_the_one_that_will_happen`

<a id="SCHED-4"></a>
### SCHED-4: a call missing a delay or a verdict is refused rather than filled in

`delay_seconds` and `noop` are both required, and nothing is scheduled without them.

**Why.** Whether a tick found anything is what the count of quiet ticks is built from, so a turn
that leaves it out is asking for a number to be invented on its behalf and shown to somebody as
an observation.

`verified-by: bravebot_agent::tools::a_schedule_missing_what_it_needs_is_refused`

<a id="SCHED-5"></a>
### SCHED-5: what the turn says it is waiting on reaches a screen and stops there

`reason` is the planner's own words, carried at the integrity of the context they came from and
released for display like any other line a tool puts on the screen. Nothing waits on it, no later
turn reads it back, and it is not sent anywhere.

`verified-by: bravebot_agent::tools::what_the_turn_is_waiting_on_reaches_the_person_watching`

<a id="SCHED-6"></a>
### SCHED-6: every turn may arrange the next look except a tick the person timed

A turn nobody is looping is offered this, and the wait it gives starts a loop over the line the
person typed ([LOOP-14](../loop.md#LOOP-14)). A tick of a self-paced loop is offered it and sets the
pace of the next tick. A tick of a loop the person gave an interval for is offered nothing, and a
call from one is answered the way any other name nobody offered is: no such tool. So is a call from
a delegate.

The confirmation says which of the two happened, because the answer the turn is writing differs: a
tick reports what this look found and leaves the rest to the next one, while a turn that has just
arranged the first later look is telling somebody a watch now exists and needs nothing from them.

**Why.** A request to be told when something changes cannot be answered inside one turn, and the
rule that withheld the tool from every turn but a tick meant the only honest answer was a single
read and a suggestion that the person arrange the rest themselves. A tick the person timed is the
one turn with nothing to decide: the interval already says when it runs, so a wait asked for there
would be dropped, and a tool present but inert is one the planner has to be told to ignore.

`verified-by: bravebot_agent::tools::any_turn_may_arrange_the_next_look_and_is_told_which_case_it_is`
`verified-by: bravebot_agent::tools::a_tick_the_person_timed_is_offered_no_way_to_schedule_one`
`verified-by: bravebot_agent::tools::a_turn_outside_a_loop_is_told_the_next_look_is_already_arranged`
`verified-by: bravebot_agent::turn::a_turn_that_is_not_a_tick_can_arrange_the_next_look`
`verified-by: bravebot_agent::turn::a_tick_the_person_timed_cannot_reschedule_itself`
`verified-by: bravebot_agent::turn::a_tick_of_a_self_paced_loop_says_when_to_run_again`
`verified-by: bravebot_agent::tools::a_delegate_is_offered_no_task_list_and_no_way_to_ask`

---
id: ARM
title: watch_file
status: normative
governs:
  - crates/agent/src/tools.rs
  - crates/agent/src/watch.rs
documented-by: none (gap: no page documents arming a watch on a file)
---

## Scope

Arming a standing watch on one file. `path` is routing, and it is the only argument. The result is
a confirmation saying the watch exists.

What a watch then is, what a firing does, how long one lives and what ends one is
[../file-watches.md](../file-watches.md). Which paths may be read at all is
[../trust-map.md](../trust-map.md).

## Clauses

<a id="ARM-1"></a>
### ARM-1: one field, the path, and nothing else

No interval, no condition, no sentence for the fire to say, and no second path. A person reading
the call sees which file this session may be told about, and that is the whole of the decision.

**Why.** This is what makes the routing approvable on its own, the test every tool on this surface
is held to. A field for what a fire should say would let a turn write its own next prompt; a field
for how often to look would make the latency the planner's to choose; and either would turn a call
anybody can read into one they cannot.

`verified-by: bravebot_agent::tools::nothing_on_this_tool_says_anything_but_which_file`

<a id="ARM-2"></a>
### ARM-2: the gate is the one a read of that path goes through, and nothing more is granted

Inside the working directory that is the promotion a read of the model's own choice of file
already gets; outside it, whatever answer already stands. A path a read would be refused is a
watch that is refused.

**Why.** A person who asked to be told when a file changes has asked for less than a read of it, so
a prompt of its own here would put a second question to somebody who has already answered the
first. And a path nobody vouched for is one this program has no business reporting movement on, so
the gate is not looser either.

`verified-by: bravebot_agent::tools::a_path_outside_the_workspace_is_refused_the_way_a_read_of_it_would_be`

<a id="ARM-3"></a>
### ARM-3: the path must name a file that exists now

A directory is refused, and so is a name with nothing at it.

**Why the directory.** What changed inside one is a file name the filesystem produced, and a fire's
prompt may carry nothing off the filesystem, so the only fire a directory could produce is one
saying it moved, which a planner can do nothing with.

**Why an existing file.** A watch compares each look with the look before, and the first look is
taken when the watch is armed. A path with nothing at it leaves nothing to compare against, so the
first look that found the file would be reported as a change the file never underwent.

`verified-by: bravebot_agent::tools::a_directory_is_refused_rather_than_watched`
`verified-by: bravebot_agent::tools::a_path_that_names_nothing_is_refused`

<a id="ARM-4"></a>
### ARM-4: a call the session cannot honour is refused with the reason, not armed silently

A session already running a loop or working towards a goal, and a session holding as many watches
as it keeps, each refuse the call and say which of the three it is. The count is read against what
this turn has already armed, so a turn arming one after another is stopped at the slot the session
actually has left.

**Why the reason.** The turn is the only thing that can tell the person, and "refused" alone leaves
it guessing between a rule it could explain and a mistake it should retry.

**Why not armed silently.** The watch lives in the session rather than here, so a confirmation this
tool wrote without the session's answer would be telling the planner about a watch that does not
exist.

`verified-by: bravebot_agent::tools::a_session_already_doing_something_untyped_refuses_and_says_which`
`verified-by: bravebot_agent::tools::a_turn_that_fills_the_last_free_slot_is_refused_after_it`

<a id="ARM-5"></a>
### ARM-5: a surface that keeps no watches is offered no way to arm one

A delegate, a one-shot run and a planned run are offered nothing, and a call from one is answered
the way any other name nobody offered is: no such tool.

**Why.** A fire is a turn nobody asked for, arriving at whoever is reading the session. A run with
nobody in front of it has no such reader, and a delegate ends when it answers, so a watch it armed
would outlive everything that could act on one.

`verified-by: bravebot_agent::tools::a_surface_that_keeps_no_watches_is_not_offered_the_tool`

<a id="ARM-6"></a>
### ARM-6: where this tool is offered, the read's description sends a question about one file to it

`read_file`'s description says that a question about whether one file changes is answered by
arming a watch, and that the wait a turn schedules is for everything else. Where no watch can be
armed, that sentence is absent and the description is what it was.

**Why.** Both answers exist wherever watches do, and both describe themselves as the answer to a
request to be told when something changes. A planner given two and told nothing about which takes
the one it read first, which is the read's own paragraph and therefore always the schedule. The
sentence is what makes the choice the driver's rather than an artefact of the order the tool
descriptions happen to be in.

**Why not the other way.** A wait the turn schedules is still the answer where what is waited on
is not one file, and where the person asked for their own line to be run again. Cutting it would
leave those cases with nothing.

`verified-by: bravebot_agent::tools::a_read_is_sent_to_the_watch_where_one_can_be_armed`
`verified-by: bravebot_agent::tools::a_read_is_sent_to_the_schedule_alone_where_no_watch_can_be_armed`

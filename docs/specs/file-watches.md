---
id: FSWATCH
title: Being told when a file changes
status: normative
governs:
  - crates/tui/src/watch_command.rs
  - crates/tui/src/state.rs
  - crates/tui/src/app.rs
  - crates/tui/src/status.rs
  - crates/agent/src/watch.rs
  - crates/agent/src/tools.rs
documented-by: docs/website/docs/using/watches.md
---

## Scope

A standing watch on one path: a thing a turn arms and that fires later, on a change, with no turn
running to notice. What a watch may observe, what a firing puts into the conversation, when it is
allowed to fire, how long one lives, what ends one, and where a live one is shown.

A self-paced loop over the line a person typed still exists and is untouched: that is
[loop.md](loop.md) and [tools/schedule-next.md](tools/schedule-next.md), and it stays exactly as it
is for a person who typed their own repeating line. A watch is the other answer, for the case where
what is being waited on is one file and nobody wants their line sent again.

The comparison a watch is built on is the change token a read hands back, which is
[tools/read-file.md](tools/read-file.md): a file's size and modification time, compared between two
looks. This document does not change it. Which paths may be read at all is
[trust-map.md](trust-map.md). What a repeating prompt is, and what ends one, is
[loop.md](loop.md), and one condition judged after every turn is [goal.md](goal.md). A watch is
neither, and a session holds one of the three at a time ([FSWATCH-6](#FSWATCH-6)).

The tool a turn arms a watch with is [tools/watch-file.md](tools/watch-file.md): one tool with one
field, a path. What this document fixes is what the driver owes whoever calls it.

## Why it exists

A request to be told when something changes cannot be answered inside one turn: the turn that reads
a file now cannot see it written later. The arrangement that ships answers it by asking again, so
the report of a change is a look taken at a tick, and its latency is the wait a turn chose rather
than the moment the file moved. Between two ticks nothing is watching. A person who asked to be
told when a build finished learns it at the next tick, or never, because the loop ends when a turn
stops re-arming it.

The gap is not the wait. It is that there is no object in this program whose job is to notice, so
the only thing that can notice is a turn, and a turn is the expensive thing.

## The rule this does not break

`crates/agent/src/exec.rs` says of a background pipeline that a job outliving the turn that started
it would be an effect nobody is watching and nobody can stop, so the turn owns it and ends it.

That rule stands where it stands, and this document does not relax it. It is about a program left
running: a pipeline has already been given a command line, it writes where it was pointed, and what
it does after its turn ends is not observed by anything. A watch runs no program. It starts nothing,
writes nothing, and reads no content ([FSWATCH-1](#FSWATCH-1)). There is no effect to be unwatched.

Read at its broadest, as an objection to any state that outlives a turn, the rule names two
properties: unwatched, and unstoppable. A watch is neither, and the clauses below are where it pays
for that: it is listed on the screen while it is live ([FSWATCH-10](#FSWATCH-10)), it can be stopped
by name and all of them by the key that stops things ([FSWATCH-9](#FSWATCH-9)), it cannot outlive a
bound nobody has to remember ([FSWATCH-8](#FSWATCH-8)), it cannot outlive the process
([FSWATCH-11](#FSWATCH-11)), and it says which turn armed it ([FSWATCH-10](#FSWATCH-10)).

## What a watch is

<a id="FSWATCH-1"></a>
### FSWATCH-1: a watch outlives the turn that armed it, and observes rather than acts

The turn that arms a watch ends, and the watch is still live. For as long as it is, it does one
thing: it looks at a path, and compares what it sees with what it saw at the look before. It starts
no process, writes no file, sends no request, and reads no byte of the file's content.

A difference between two consecutive looks is what arms a fire. The first look is taken when the
watch is armed, so a change is a change since somebody asked about the file, and every look
afterwards is measured against the one before it rather than against that first one.

**Why.** This is the whole of what is being asked for, and it is also the whole of what may be
allowed. A thing that outlives a turn and can only look is answerable to the bounds below; a thing
that outlives a turn and can act would have to be watched by something, and there is nothing running
to watch it.

`verified-by: bravebot_agent::watch::a_change_seen_between_two_looks_makes_a_fire_due`
`verified-by: bravebot_agent::watch::a_look_that_sees_what_the_last_one_saw_fires_nothing`
`verified-by: bravebot_agent::watch::a_look_is_measured_against_the_one_before_it_rather_than_against_the_first`
`verified-by: bravebot_agent::watch::the_look_taken_when_a_watch_is_armed_is_what_the_next_one_is_measured_against`
`verified-by: bravebot_tui::state::a_change_begins_a_turn_with_no_turn_running_to_notice_it`

<a id="FSWATCH-2"></a>
### FSWATCH-2: a watch names one path, settled when it is armed

One path, and it never changes for the life of the watch. Not a glob, not a directory, and not a
tree walked for anything underneath it.

**Why.** A watch on a directory has to say what changed inside it to be worth anything, and what
changed inside it is a file name the filesystem produced rather than a name anybody in this
conversation wrote. Reporting that name is untrusted content in a synthesized prompt, which is what
[FSWATCH-4](#FSWATCH-4) exists to prevent, and reporting only that the directory moved is a fire a
planner can do nothing with. Refusing the case at the surface is cheaper than labelling a name that
has no business being in a prompt at all.

A path fixed at arming time is also what makes a fire's prompt free of anything new: every word of
it was already in the context of the turn that armed the watch.

`verified-by: bravebot_agent::tools::a_directory_is_refused_rather_than_watched`
`verified-by: bravebot_tui::status::the_report_lists_every_live_watch_with_the_turn_that_armed_it`

<a id="FSWATCH-3"></a>
### FSWATCH-3: what a watch observes is the file's size and modification time, and nothing else

The same two facts a read hands the planner back as a change token, compared the same way. No
content, and nothing derived from content: not a hash, not a first line, not a byte count of what
differs.

The driver holds those two facts, because comparing them is the whole of what a watch does. Nothing
about them reaches the planner. A fire says that a path looks written to since the last look
([FSWATCH-4](#FSWATCH-4)), which is the strongest thing it can say, and it says it without quoting
a size or a modification time.

**Why no content.** A file the trust map quarantines is one whose bytes may not reach the planner,
and a watch that derived a bit from those bytes would be releasing content-derived information
about exactly such a file, which is what that arrangement exists to prevent. Size and modification
time are facts about the file rather than facts from inside it.

**Why the modification time stays with the driver.** This program does not hand the planner a time
of day, which [tools/run.md](tools/run.md) states and the change token a read hands back is built
to respect. A fire that carried the moment the file moved would be a clock, and it would invite
exactly the invented timestamps that rule exists to stop.

An identical rewrite moves the two facts and so fires a watch, a change and a change back between
two looks moves nothing and so fires nothing, and a filesystem that leaves a modification time
alone hides a change entirely. These are properties of the comparison rather than of this watch,
and [tools/read-file.md](tools/read-file.md) states them for the read that shares it.

`verified-by: bravebot_agent::watch::a_look_that_sees_what_the_last_one_saw_fires_nothing`
`verified-by: bravebot_agent::watch::a_fires_prompt_carries_the_watch_and_the_path_and_nothing_off_the_filesystem`

## What a firing does

<a id="FSWATCH-4"></a>
### FSWATCH-4: a fire's prompt says which watch fired and on what path, and carries nothing from the filesystem

A fire begins a turn by putting a line in the conversation in the user's own role. That line is the
driver's own sentence, and the only things in it that vary are which watch fired and the path that
watch was armed on. No file content, no size, no modification time, no directory listing, and no
name the filesystem produced.

The turn then asks for the content if it wants it, and the content arrives labelled, through the
read that every other path into this conversation goes through.

**The path in that sentence is prose and vouches for nothing.** No file is opened by it, and
writing it as `@path` would not make it an endorsement: a keystroke is what makes naming a file
an endorsement, and there is no keystroke behind a sentence this program wrote. The same rule the
sentence carrying a goal on is held to, for the same reason, and [goal.md](goal.md) states it
there.

**Why the sentence carries structure only.** The role a synthesized prompt lands in is the one the
model trusts most, so a prompt naming a watched file's contents would launder untrusted bytes
straight into it, past every gate that exists for them. There is no label to attach in that
position and no way to quarantine a sentence, so the only safe content for the sentence is content
that was never untrusted.

**Why a path is not that.** Two reasons, and both are needed. The path was written by the turn that
armed the watch, out of a context that holds no untrusted content, so it is not untrusted bytes
arriving in a trusted position. And it cannot have changed since ([FSWATCH-2](#FSWATCH-2)), so a
fire hours later carries the same string a reader could see in the conversation above it. What the
path is not is an instruction: the sentence around it is the driver's, and a planner that could
write the rest of the sentence would be writing its own next prompt, which [loop.md](loop.md)
refuses for a repeated line and this refuses for the same reason.

**Why not more, since a fire costs a turn anyway.** The saving would be one read. The cost is that
the one prompt in this system nobody can label would be the one carrying bytes off a disk.

`verified-by: bravebot_agent::watch::a_fires_prompt_carries_the_watch_and_the_path_and_nothing_off_the_filesystem`
`verified-by: bravebot_agent::watch::a_fires_prompt_does_not_endorse_the_path_it_names`
`verified-by: bravebot_tui::state::a_fires_prompt_carries_the_watch_and_the_path_and_nothing_else`

<a id="FSWATCH-5"></a>
### FSWATCH-5: a fire waits for an idle session and never interrupts, and changes while a turn runs are one fire

A watch that has seen a change fires when the session is idle and nothing the person queued is
still waiting, and not before. Any number of changes seen while a turn is running, or while a fire
is held for one, produce one fire when the session is free.

**Why.** A filesystem event is not a licence to interrupt: the person is still the one using this
session, and the same holds for a tick of a loop. Coalescing follows from what a fire can say: it
reports that the path looks written to, which is one fact however many times it was written, so a
queue of held fires would be several turns all reporting the same sentence.

`verified-by: bravebot_tui::state::a_fire_waits_for_the_turn_in_flight_and_for_what_is_queued`
`verified-by: bravebot_agent::watch::changes_seen_before_a_fire_goes_out_are_one_fire`
`verified-by: bravebot_agent::watch::a_watch_whose_fire_is_running_is_not_due_again`
`verified-by: bravebot_tui::app::a_fire_whose_turn_failed_stops_being_the_turn_in_flight`

<a id="FSWATCH-6"></a>
### FSWATCH-6: a watch, a loop and a goal are never live together, and a watch asked for under one is refused

A session does one of the three at a time. A turn that would arm a watch while a loop is running or
a goal is set arms nothing and says why, and a person who starts a loop or sets a goal while
watches are live is told that they have ended. Several watches may be live together, up to the
count in [FSWATCH-8](#FSWATCH-8): the rule is about kinds, not about watches.

**Why.** All three make a session work without anybody typing, and the reason a loop and a goal
already exclude each other is that each spoils the other's meaning. A goal is judged after every
turn and has a fixed number of rounds, so fires would spend a budget the person set aside for the
work, and the condition would be judged against a turn that was reporting a file rather than doing
anything towards it. A loop is no better served: its interval is what the person asked for, and a
watch firing between ticks makes the interval a floor rather than a pace.

**Why a turn is refused where a person is not.** A person typing `/loop` or setting a goal is
present and means it, so their second request stands and the watches end saying so. A watch is
asked for by a turn, and a turn that silently took somebody's goal off would be ending work they
are waiting on in order to watch a file.

`verified-by: bravebot_tui::state::a_watch_asked_for_under_a_loop_or_a_goal_is_refused_and_says_why`
`verified-by: bravebot_tui::state::a_person_starting_a_loop_or_a_goal_is_told_the_watches_have_ended`
`verified-by: bravebot_agent::tools::a_session_already_doing_something_untyped_refuses_and_says_which`
`verified-by: bravebot_tui::state::a_later_look_a_turn_asked_for_is_refused_while_a_watch_is_live`

<a id="FSWATCH-7"></a>
### FSWATCH-7: a watch is armed only where the path could have been read, and ends if that stops being true

Arming a watch goes through the gate reading the file goes through, and grants nothing beyond it:
inside the working directory that is a promotion nobody is asked about, and outside it that is
whatever the trust map answers. Where the answer that allowed it stops holding, the watch ends and
says so rather than continuing to look.

**Why the same gate and not a looser one.** Size and modification time are not content, and a
watch is still a standing channel about a path. A path nobody vouched for is one this program has
no business reporting movement on.

**Why not a stricter one.** A person who asked to be told when a file changes has asked for less
than a read of it, and a watch that had its own prompt would put a second question to somebody who
has already answered the first.

**Why it ends when the answer stops holding.** A watch outliving its own permission is a way to
keep a question alive past the moment it was agreed to, and the answers this program keeps do
expire.

`verified-by: bravebot_agent::tools::a_path_outside_the_workspace_is_refused_the_way_a_read_of_it_would_be`
`verified-by: bravebot_agent::watch::a_path_the_session_no_longer_reaches_ends_its_watch_and_says_so`
`verified-by: bravebot_tui::state::a_watch_that_ends_itself_says_which_of_the_two_endings_it_was`

## The bounds

<a id="FSWATCH-8"></a>
### FSWATCH-8: how long a watch lives, how many there may be, and how often one may fire

| Bound | Value |
|---|---|
| a watch's age | 7 days, after which it ends itself and says so |
| live watches in one session | 8, and arming a ninth is refused rather than dropping one |
| between two fires of the same watch | 5 seconds, measured from the end of the turn the last fire started |
| between two looks at a watched path | at most 5 seconds, which is what a fire's latency is |

**Why an age at all, given it cannot outlive the session.** A session left open for a week is a
session nobody is sitting at, and a watch armed on the first day of one is a prompt arriving with
no cause anybody present remembers. The number is the one a loop already uses, and there is no case
for two different ceilings on the same kind of thing.

**Why a count.** Each live watch is a fire that can happen, and a person reading fires is the point
of the feature. Eight is more paths than a conversation names and few enough that a tree of files
all being written cannot produce a queue nobody can read.

**Why the gap is measured from the end of the turn.** A fire is a whole turn, so the turn's own
length is what spaces fires out, exactly as it is for a tick. The floor is not what makes a watch
slow; it is there so that a file being written continuously cannot become a session that is
continuously in a turn.

**Why looking is not instant.** A change is noticed by looking, so the latency is how often a look
happens rather than how fast the filesystem is. Five seconds is short enough that a person who
saved a file sees the fire as a consequence of saving it.

`verified-by: bravebot_agent::watch::a_watch_older_than_a_week_ends_itself_and_says_so`
`verified-by: bravebot_agent::watch::a_ninth_watch_is_refused_rather_than_dropping_one`
`verified-by: bravebot_agent::watch::a_second_fire_waits_for_the_floor_after_the_last_ones_turn`
`verified-by: bravebot_agent::watch::a_path_is_not_looked_at_again_until_the_interval_is_up`
`verified-by: bravebot_tui::state::a_session_holding_as_many_watches_as_it_keeps_reports_itself_full`

<a id="FSWATCH-9"></a>
### FSWATCH-9: seven things end a watch, and each of them says so

| What | When |
|---|---|
| the person ends one | `/watch stop <n>`, naming the number the report gives it, which leaves the others |
| the person interrupts | Ctrl-C with nothing nearer to stop, which ends every live watch |
| a fire's turn is stopped | the watch that fired ends with the turn it started, and the others stand |
| the person asks for a loop or a goal | every live watch ends, since a session does one of the three at a time |
| the path stops being readable | the answer that armed it no longer holds |
| the session moves on | `/clear`, and leaving |
| age | 7 days after it was armed |

Ctrl-C means one thing at a time, and the watches are the last rung before leaving: a mode open
over the session, then the turn in flight, then the half-written line, then the loop or the goal,
then every live watch, then leaving. Each is nearer than the next, and the press that ends the
watches is the one made with nothing running and nothing half written.

**Why stopping a fire's turn ends the watch that fired it.** Otherwise the key never reaches a
watch that is firing often: every press lands on a turn, and the next fire arrives seconds later.
Stopping the turn a fire started is also the most exact way anybody has to say which watch they
have finished with, since they are reading its prompt when they press the key. A turn that was not
a fire ends no watch: that press is a person steering their own work.

**Why Ctrl-C with nothing nearer to stop takes all of them.** Somebody pressing the key that stops things
wants the things stopped, and picking which of eight survived is not a decision to make from a
keystroke.

**Why each of them says so.** A watch that ended in silence is indistinguishable from a watch that
is live and has seen nothing, and the difference between those two is the whole of what a person
armed it to learn.

`verified-by: bravebot_tui::state::a_watch_is_ended_by_the_number_the_report_gave_it`
`verified-by: bravebot_tui::state::stopping_a_fires_turn_ends_the_watch_that_fired`
`verified-by: bravebot_tui::state::stopping_a_turn_that_was_not_a_fire_ends_no_watch`
`verified-by: bravebot_tui::state::a_person_starting_a_loop_or_a_goal_is_told_the_watches_have_ended`
`verified-by: bravebot_tui::state::a_watch_that_ends_itself_says_which_of_the_two_endings_it_was`
`verified-by: bravebot_tui::state::clearing_a_session_ends_every_watch`
`verified-by: bravebot_tui::app::interrupting_ends_every_watch_before_it_leaves`

<a id="FSWATCH-10"></a>
### FSWATCH-10: a live watch is on the screen, with the turn that armed it

`/status` lists every live watch: a number that names it, the path, which turn armed it, and how
long it has left. A session with no live watch says nothing about watches. A fire announces itself,
naming the watch it came from.

The number is what a person ends one by ([FSWATCH-9](#FSWATCH-9)), so it is stable for the life of
the watch: watches are numbered in the order they were armed, and a number is not reused when the
watch it named ends.

**Why.** What is going to happen without anybody typing anything is the one thing about a session
that cannot be read back off the transcript, and a thing that outlives a turn is only answerable if
it can be seen. The turn that armed it is on the line because a prompt arriving hours later is
otherwise causeless: the person reads it against the conversation, and the conversation is where
they asked for it.

`verified-by: bravebot_tui::status::the_report_lists_every_live_watch_with_the_turn_that_armed_it`
`verified-by: bravebot_tui::status::a_session_watching_nothing_says_nothing_about_watches`
`verified-by: bravebot_agent::watch::a_number_is_not_reused_when_the_watch_it_named_ends`
`verified-by: bravebot_agent::watch::a_live_watch_reports_its_path_the_turn_that_armed_it_and_what_is_left`
`verified-by: bravebot_tui::state::a_session_with_no_watch_says_so_when_asked`

<a id="FSWATCH-11"></a>
### FSWATCH-11: a watch is never written down, and the process ending is what reaps it

A watch is not in the session record. Resuming a session restores no watch, and nothing on disk
survives a run to be cleaned up by the next one.

A watch is state inside this process and is looked at by the interface's own pass, so there is no
thread, no child and no timer to reap: the process ending is the reaping, and that holds on the
crash path for the same reason it holds on the ordinary one.

**Why nothing survives the session.** A watch that outlived it would start sending prompts at
somebody who opened a conversation to read it, with no visible cause and nothing in the transcript
to explain it. The same argument [loop.md](loop.md) makes about a schedule holds about a watch, and
more strongly: a loop at least repeats a line its person typed, while a fire is a sentence this
program wrote about a file that moved while nobody was here.

**Why this is also the cheapest answer.** Every bound above is a property of one thing in one
process. A watch that was a thread, a child process or an entry in a file would need its own way to
be enumerated, stopped, aged out and reaped after a crash, and each of those is a mechanism that
can be wrong.

`verified-by: bravebot_tui::state::clearing_a_session_ends_every_watch`
`verified-by: by-construction (the watches are a field on the live session and are not among the things a session record writes, so there is nothing on disk for a resume to restore and nothing for a later run to clean up)`

<a id="FSWATCH-12"></a>
### FSWATCH-12: a path with nothing at it is refused rather than watched for something to appear

Arming asks for the first look at once, and a path that cannot be looked at is refused saying so.

**Why.** Every look after the first is compared with the one before it, and the first is compared
with nothing. A watch armed on a path with nothing at it would report the look that first found
the file as a change, which is a write it never saw and cannot have seen.

**Why not the other answer.** Watching for a file to appear is a real thing to want, and it is a
different feature: what it compares is presence rather than the two facts
[FSWATCH-3](#FSWATCH-3) fixes, and it needs its own answer to what a fire says about a file that
came and went between two looks. Refusing here leaves room for it rather than half-building it.

`verified-by: bravebot_agent::tools::a_path_that_names_nothing_is_refused`
`verified-by: bravebot_agent::watch::a_path_that_cannot_be_looked_at_is_refused_rather_than_armed`
`verified-by: bravebot_tui::state::a_path_that_cannot_be_looked_at_is_refused_and_said_so`

## Open questions

- Whether a person may arm a watch themselves, with a command, rather than only by asking for one in
  a prompt. `/watch` lists them and ends one and arms none. Nothing here needs a planner, and a
  person who wants to be told when a file changes is the one case where the whole feature costs a
  turn only at the moment it fires.
- Whether the five-second look should become an operating-system notification. The observable
  difference is latency, and the cost is a dependency and a per-platform surface, so it is a
  question about the bound rather than about any clause here.
- Whether a watch and a loop should be allowed together after all. The exclusion costs the person
  who wants a slow repeating line and a watch at the same time, which is a reasonable thing to
  want, and the argument against it is about the interval losing its meaning rather than about
  anything unsafe. A goal is the harder case of the two, because its rounds are a budget a fire
  would spend.

## Known costs

- **A fire spends a turn.** Every fire is a whole turn with the conversation re-sent, and the
  bounds hold the rate rather than the total: eight watches on files that are written all day is a
  session that is continuously in a turn, at five seconds a fire, and nothing here bounds what that
  costs.
- **Size and modification time answer a narrower question than the one people ask.** A watch says a
  path looks written to. It does not say what changed, it fires on a write that changed nothing, and
  a filesystem that does not move a modification time hides a change from it entirely. A turn that
  reports a fire as a changed file is claiming more than the fire said, and nothing mechanical
  stops it.
- **A five-second look is a five-second window.** A file written and written back inside one window
  is a change nothing reports, because the two facts compared are the same both times.
- **Nothing survives the session, by choice.** The person who wants to know when a file changes
  overnight is not served, and the answer they get is to leave the session open. That is a real gap
  and it is the price of a watch that cannot arrive at a conversation nobody armed.
- **Inside the working directory, nobody is asked at all.** A read there is a choice promoted rather
  than a question put to a person, which [tools/read-file.md](tools/read-file.md) states, and a
  watch asks exactly what a read asks. So a week-long channel about a path can be opened by a
  promotion, on a path the person may never have named. The screen and the bounds are the whole of
  what answers for it, which puts the guarantee on somebody reading `/status`, and it is the
  strongest argument for letting a person arm one themselves.

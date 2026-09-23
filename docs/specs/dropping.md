---
id: DROP
title: Dropping a file on the window
status: normative
governs:
  - crates/tui/src/dropped.rs
  - crates/tui/src/app.rs
  - crates/agent/src/attached.rs
  - crates/ui-bridge/src/bridge.rs
documented-by: docs/website/docs/using/context.md
---

## Scope

What happens when a person drags a file onto a window, and on what footing it enters the turn.
What the box does with the marker afterwards is [terminal-input.md](terminal-input.md). The
terminal is where the gesture is handled and most of this describes it; a front end reaching the
same grant over the protocol is [DROP-10](#DROP-10).

Three gestures put content into a turn on the user's own footing, and each has its own spec:
[naming-files.md](naming-files.md) for `@` in a prompt, [pasting.md](pasting.md) for Ctrl-V, and
[dropping.md](dropping.md) for a file dragged onto the window.

## Why a dropped file is trusted

Because a person picked that one file and let the line go. It enters the turn on the footing of the
words typed beside it: the path came from a gesture rather than from anything a model said, and
sending the line is the grant. Nothing inspects the contents and nothing could.

What makes reaching outside the workspace sound is not where the file sits but where its path came
from. A dropped file's path is fixed before the turn starts, and nothing a model says or a file
contains can put one there.

## Clauses

<a id="DROP-1"></a>
### DROP-1: only a file a person dropped

Never a path a model proposed, never one read out of a file, never one a processor produced. The
justification cannot be checked from the bytes, so it lives at the call site. Two call sites mint
the grant: the terminal's drop handling, and the `dropped` list a front end sends with `turn.send`,
whose caller owes what [DROP-10](#DROP-10) states. Nothing else mints it.

`verified-by: bravebot_tui::drop::dropping_an_image_puts_a_marker_in_the_line`
`verified-by: bravebot_agent::workspace::an_untrusted_path_is_not_read_as_a_drop`
`verified-by: bravebot_agent::workspace::an_untrusted_path_is_not_attached_as_a_drop`

<a id="DROP-2"></a>
### DROP-2: a drop makes that file trusted

The rule recorded is for the file itself, so its contents can be read and it can be edited for the
rest of the session. A rule on a file is more specific than any rule on the tree around it, so a
dropped file is trusted even inside a directory marked untrusted.

`verified-by: bravebot_agent::turn::attaching_a_file_vouches_for_it_the_way_naming_one_does`
`verified-by: bravebot_agent::attached::a_dropped_picture_is_shown_even_from_a_directory_nobody_vouched_for`
`verified-by: bravebot_agent::attached::a_drop_records_its_rule_in_the_callers_map`
`verified-by: bravebot_agent::attached::a_drop_before_one_that_could_not_be_read_keeps_its_rule`
`verified-by: bravebot_agent::manifest::a_dropped_picture_is_still_trusted_when_a_step_of_the_plan_reads_it`

<a id="DROP-3"></a>
### DROP-3: a drop makes that file reachable, wherever on the disk it is

A dropped file, and only a dropped file, may name a path outside the working directory. Whether it
is carried as bytes or read as text makes no difference, since the same gesture produced both.

Both grants are for the one file. Nothing else in the directory it came from becomes trusted or
reachable, and reading, writing, editing, listing and searching stay confined exactly as they
were.

**Why.** A drop can come from anywhere on the disk and usually does, because the place someone
drags a file from is rarely inside the project they are working on. Confining a drop to the
workspace would refuse the ordinary case. What makes reaching out sound is not where the file sits
but that a person's gesture put its path there.

`verified-by: bravebot_tui::drop::a_drop_from_outside_the_workspace_is_attached_all_the_same`
`verified-by: bravebot_tui::drop::a_text_file_from_outside_the_workspace_is_dropped_all_the_same`
`verified-by: bravebot_tui::drop::the_name_handed_to_the_task_is_relative_to_the_workspace`
`verified-by: bravebot_agent::workspace::a_dropped_text_file_may_come_from_outside_the_workspace`
`verified-by: bravebot_agent::workspace::a_dropped_attachment_may_come_from_outside_the_workspace`
`verified-by: bravebot_agent::workspace::only_a_dropped_attachment_may_come_from_outside_the_workspace`
`verified-by: bravebot_agent::attached::a_picture_dropped_from_outside_the_workspace_is_read_all_the_same`
`verified-by: bravebot_agent::turn::a_dropped_text_file_from_outside_the_workspace_becomes_context`
`verified-by: bravebot_agent::turn::dropping_a_text_file_does_not_reach_anything_beside_it`

<a id="DROP-4"></a>
### DROP-4: a recognised type is carried, and an unrecognised one is only a path

Images and PDFs are carried as bytes, so the model looks at them. A text file becomes context, its
contents entering the turn as trusted input. A type nothing here takes has its path written into the line instead, which is
what dropping a file did before any of this existed. Extensions are recognised whatever their case.

A slash command whose argument reaches a model carries what such a request can hold. `/btw` sends a
question and `/manifest` plans from a task, and a picture or a PDF dropped onto either goes with the
words in the one message. Neither planner reads it: the bytes are read before the planner's policy
exists, under a policy holding only the read and only the paths the gesture fixed, so a planner that
cannot reach a file is handed bytes rather than given a way to reach one. A text file on one of those
lines becomes its name instead, because its contents would be a context message and both of those
planners are precommitted to a context holding the task and the driver's own words. `/loop` carries
both, since its first tick is an ordinary turn; every tick after it sends each marker as the file's
name, because the file went with the tick that took it. A slash command the driver carries out
itself has no request for anything to travel in, so every marker in one becomes the file's name.
Waiting changes none of this, so a command the queue reaches when a turn ends carries what one
dispatched at rest carries.

`verified-by: bravebot_tui::drop::a_dropped_text_file_is_context_rather_than_an_attachment`
`verified-by: bravebot_tui::drop::dropping_an_unsupported_type_writes_out_the_path`
`verified-by: bravebot_tui::dropped::an_unsupported_type_is_a_drop_that_attaches_nothing`
`verified-by: bravebot_tui::dropped::an_unsupported_file_beside_a_supported_one_leaves_it_attachable`
`verified-by: bravebot_tui::dropped::an_extension_is_recognised_whatever_its_case`
`verified-by: bravebot_tui::dropped::the_recognised_types_are_the_ones_claude_code_takes`
`verified-by: bravebot_tui::app::a_picture_dropped_onto_a_question_goes_with_it`
`verified-by: bravebot_tui::app::a_picture_dropped_onto_a_task_goes_with_the_plan`
`verified-by: bravebot_tui::app::a_picture_dropped_onto_a_loop_goes_with_its_first_tick`
`verified-by: bravebot_tui::app::a_picture_dropped_onto_a_question_that_waited_still_goes_with_it`
`verified-by: bravebot_tui::app::a_text_file_dropped_onto_a_question_is_sent_as_its_name`
`verified-by: bravebot_tui::app::a_file_dropped_onto_a_command_line_is_carried_out_as_its_name`
`verified-by: bravebot_tui::state::a_later_tick_of_a_loop_names_the_file_the_first_one_carried`
`verified-by: bravebot_tui::loops::a_dropped_file_goes_to_one_tick_and_its_name_to_every_other`
`verified-by: bravebot_agent::attached::a_dropped_picture_comes_back_as_the_bytes_a_request_can_hold`
`verified-by: bravebot_agent::attached::reading_a_dropped_picture_is_gated_and_named_in_the_trail`
`verified-by: bravebot_agent::attached::a_picture_that_is_not_there_fails_rather_than_being_carried_as_nothing`
`verified-by: bravebot_agent::attached::two_dropped_pictures_come_back_in_the_order_their_markers_number_them`
`verified-by: bravebot_agent::turn::a_picture_dropped_onto_a_question_reaches_the_model_with_it`
`verified-by: bravebot_agent::manifest::a_picture_dropped_onto_the_task_reaches_the_planner`

<a id="DROP-5"></a>
### DROP-5: dropping a directory attaches nothing

A directory is somewhere to type through rather than a file to read, so naming one includes nothing.

`verified-by: bravebot_tui::drop::dropping_a_directory_attaches_nothing`

<a id="DROP-6"></a>
### DROP-6: each dropped file gets its own marker, and deleting one takes it off

`[Image #1]`, numbered so a second drop is distinguishable from the first, each keeping its place in
a mixed drop. Deleting the marker is the only way to change your mind, and sending the line clears
what was attached to it. Dispatching a slash command counts as sending it: the line comes off the
box with what it named, so nothing is left staged behind a box that no longer names it.

`verified-by: bravebot_tui::drop::several_files_dropped_together_each_get_a_marker`
`verified-by: bravebot_tui::drop::a_second_drop_gets_its_own_number`
`verified-by: bravebot_tui::drop::a_mixed_drop_keeps_each_in_its_place`
`verified-by: bravebot_tui::drop::deleting_the_marker_takes_the_attachment_off`
`verified-by: bravebot_tui::drop::sending_a_line_clears_what_was_attached_to_it`
`verified-by: bravebot_tui::app::dispatching_a_command_clears_what_was_dropped_on_its_line`
`verified-by: bravebot_tui::app::a_command_line_whose_dropped_marker_was_deleted_carries_no_file`
`verified-by: bravebot_tui::drop::a_drop_leaves_room_after_itself`

<a id="DROP-7"></a>
### DROP-7: a line is a drop only when every word of it is a path that exists

Terminals deliver a drop as text, so it has to be told from typing. A plain, quoted, backslash
escaped or `file://` path counts, several at once count, and a percent sign in a name survives. One
word of prose, a path naming nothing, an unterminated quote, an empty paste, or more than one line
makes it a paste instead.

**Why.** Guessing wrong in the permissive direction would attach a file because somebody mentioned
its name, which is a path nobody's gesture put there.

`verified-by: bravebot_tui::dropped::a_plain_path_is_a_drop`
`verified-by: bravebot_tui::dropped::a_quoted_path_is_unquoted`
`verified-by: bravebot_tui::dropped::a_backslash_escaped_path_is_unescaped`
`verified-by: bravebot_tui::dropped::a_file_uri_becomes_a_path`
`verified-by: bravebot_tui::dropped::a_literal_percent_in_a_name_survives`
`verified-by: bravebot_tui::dropped::several_files_dropped_at_once_are_all_taken`
`verified-by: bravebot_tui::dropped::prose_mentioning_a_real_file_is_not_a_drop`
`verified-by: bravebot_tui::dropped::a_path_that_names_nothing_is_not_a_drop`
`verified-by: bravebot_tui::dropped::one_word_of_prose_is_enough_to_make_it_a_paste`
`verified-by: bravebot_tui::dropped::a_multi_line_paste_is_never_a_drop`
`verified-by: bravebot_tui::dropped::an_unterminated_quote_is_not_a_drop`
`verified-by: bravebot_tui::dropped::an_empty_paste_is_not_a_drop`
`verified-by: bravebot_tui::drop::pasting_prose_about_a_real_file_is_still_prose`

<a id="DROP-8"></a>
### DROP-8: a file dropped onto a line queued mid-turn is named, not carried

A prompt sent while a turn is running goes into that running turn rather than waiting for the next
one ([terminal-input.md](terminal-input.md)), and a turn already running cannot be handed a file: it
precommitted its routing and fixed the shape of its context before it read anything, so there is no
trusted slot for one to arrive in. What the planner is given instead is the file's
**name**, in place of the marker, settled at the moment the line was sent. A pasted picture has no
such recourse and resolves to a sentence saying a picture was pasted and cannot be shown.

Only what talks to the model sees this. The box and the transcript keep the marker, because that
is what was on the person's screen.

**Why.** Dropping a file mid-turn is telling the agent which file to look at, and a name is enough
for that: the planner reads it and goes to the file through the same gate it reads any other file
through, with the trust map deciding as it always does. The two alternatives are worse. Leaving the
marker in place sends `[Image #1]`, which stands for nothing the planner can use. Admitting the file
would mean minting a trusted path in the middle of a turn from a string that crossed a thread
boundary after untrusted content had already been observed, which is the property
[routing.md](routing.md) exists to hold.

A prompt that is still waiting when the turn ends is not affected: it becomes a turn of its own, and
as its own turn it carries its files and pictures the way any prompt does.

`verified-by: bravebot_tui::drop::a_file_dropped_into_a_queued_line_is_named_to_the_planner`
`verified-by: bravebot_tui::drop::resolving_a_queued_line_does_not_rewrite_what_the_person_sees`

<a id="DROP-9"></a>
### DROP-9: a recalled prompt names the file rather than the marker

What the history remembers is the line with each dropped file's name in place of the marker it
was sent behind, so a prompt recalled in a later session says which file it was about.

**Why.** A marker stands for a file staged beside the line, and nothing staged outlives the
session that staged it. Remembered as it stands, `[Image #1]` comes back naming nothing: the
person reads a prompt that claims to carry a file, and the planner is sent a placeholder standing
for one. A name is what is left when the staging is gone, and it is enough: the planner reads it
and goes to the file through the same gate it reads any other file through, which is what a file
dropped onto a line queued mid-turn already relies on.

`verified-by: bravebot_tui::drop::a_dropped_file_is_recalled_by_name_rather_than_by_its_marker`

<a id="DROP-10"></a>
### DROP-10: a front end sending a drop over the protocol accounts for the path

`turn.send` carries a `dropped` list, and every path in it is read the way a text file dropped onto
the terminal is: an unconfined read, and a rule in the session's trust map. That and nothing else.
The list carries no type of its own, so [DROP-4](#DROP-4) is the terminal's alone and a picture
named here fails the turn rather than reaching the model as bytes.

The caller is a separate process, so the gesture is not visible to the bridge and what the caller
sends is the whole of the justification. A front end putting a path there is saying that a person's
gesture produced it, or that the file is one the front end composed itself out of what it already
speaks for.

What the bridge decides is the half a string can be held to: each entry is an absolute path naming
a file that is there. A directory is refused, on [DROP-5](#DROP-5)'s reasoning. A path naming
nothing is refused, on [DROP-7](#DROP-7)'s: guessing in the permissive direction admits a path
nobody's gesture put there. A relative path is refused because `files` is the list for a path
inside the project, and the two lists differ in nothing else a caller can see, so admitting one
would mint the unconfined grant for a file its caller meant as an ordinary one. An operating system
reports a drop as an absolute path, so a front end loses nothing by it.

**Why a refusal rather than an entry left out.** A turn that lost the file it was sent with is not
a smaller turn: it answers without what it was asked about, and reports success. The caller knows
what the path was for and can say so; the turn cannot.

`verified-by: bravebot_ui_bridge::dispatch::a_dropped_path_that_names_no_file_is_refused`
`verified-by: bravebot_ui_bridge::dispatch::a_relative_dropped_path_is_refused`
`verified-by: bravebot_ui_bridge::dispatch::a_turn_may_name_files_or_none_and_none_is_the_default`

## Known costs

- **A front end has no way to carry a picture over the protocol.** The `dropped` list is read as
  text and nothing beside it takes bytes, so a graphical front end that wants a dropped screenshot
  in front of the model has nowhere to put it, and naming it in the list ends the turn in an error
  about binary content. What the terminal does with a recognised type has no counterpart here.
- **A front end's word is the whole of a protocol drop's justification.** The checks
  [DROP-10](#DROP-10) puts on a `dropped` entry establish that a file is there, not that anybody
  dragged it: a path the caller invented and a path a person dropped are the same string on the
  wire, and no check over a pipe tells them apart. A front end that mints one carelessly grants
  its session a trusted, unconfined read of that file, which is the grant the terminal's gesture
  buys and the only thing the bridge cannot ask for evidence of.
- **A screenshot somebody sent you is content you have not read and are vouching for.** It goes
  into the turn as trusted input, on the strength of the gesture alone. Be as careful about a drop
  as about answering yes to a directory.
- **A name is only as useful as the reach the request has.** A file the planner is given the name of
  rather than the contents of is one it has to read for itself, and a request with no read in it
  cannot, so a text file dropped onto `/btw` or `/manifest` reaches the planner as a path it can
  only talk about. The same holds for a name that points outside the workspace, which is where a
  drop usually comes from: a later tick of a loop, a prompt recalled in a new session and a line
  queued mid-turn all hand over such a path, and confinement refuses the read when something acts
  on it. What the alternative costs is the reason: admitting the contents means a context message,
  and the requests that would need one fixed the shape of their context before the line was sent.
- **A line that both pastes and drops sends its pictures in one order and numbers them in
  another.** Every request puts the dropped files first and the pasted pictures after them, while
  one counter numbers the markers in the order the gestures happened, so `[Image #1]` from a paste
  and `[Image #2]` from a later drop arrive the other way round. A picture travels as bytes with
  nothing beside it to say which marker it answers, so the planner has the order and nothing else.
  Doing better means a part per marker, which is a shape the model API does not offer for a picture.

---
id: INPUT
title: The input box
status: normative
governs:
  - crates/tui/src/app.rs
  - crates/tui/src/input.rs
  - crates/tui/src/state.rs
  - crates/tui/src/wrap.rs
  - crates/tui/src/editor.rs
  - crates/tui/src/history_search.rs
  - crates/tui/src/vim.rs
  - crates/tui/src/config_prompt.rs
  - crates/tui/src/keybindings.rs
  - crates/config/src/settings.rs
documented-by: docs/website/docs/using/interactive-mode.md
---

## Scope

What the user types into: how the box behaves, which keys do what, what the session takes from the
terminal to make any of it work, and where a terminal's own limits show through. What is drawn back is [terminal-transcript.md](terminal-transcript.md), and
pasting is [pasting.md](pasting.md).

## Clauses

<a id="INPUT-1"></a>
### INPUT-1: the box grows with the text, up to a cap

The cap is ten rows, enough for a substantial paragraph while the transcript keeps the majority of
a standard terminal. Beyond it the box scrolls to the cursor rather than growing further, and it
keeps growing while a turn runs. A long list of indicators leaves room for the box and the
transcript, and the box stays beneath the list.

**Why.** The line being composed is the one thing a person must always be able to see.

`verified-by: bravebot_tui::render::the_input_box_grows_with_the_text`
`verified-by: bravebot_tui::render::the_input_box_stops_growing_at_the_cap`
`verified-by: bravebot_tui::render::a_very_long_input_scrolls_to_the_cursor`
`verified-by: bravebot_tui::render::the_box_grows_mid_turn_too`
`verified-by: bravebot_tui::render::a_long_list_leaves_room_for_the_box_and_the_transcript`
`verified-by: bravebot_tui::render::the_box_stays_beneath_the_list`


<a id="INPUT-2"></a>
### INPUT-2: Shift-Enter starts a line, Enter sends

Ctrl-J does the same thing everywhere and needs no terminal configuration, because most terminals
send the same byte for Enter whichever modifier was held. Both work while a turn runs and in shell
mode. A newline lands at the caret, does not arm shell mode, and Enter still sends a paragraph
written this way. Enter on an empty line does nothing.

`verified-by: bravebot_tui::app::shift_enter_starts_a_line_instead_of_sending`
`verified-by: bravebot_tui::app::ctrl_j_starts_a_line_too`
`verified-by: bravebot_tui::app::ctrl_j_is_not_swallowed_while_a_turn_runs`
`verified-by: bravebot_tui::app::shift_enter_works_in_shell_mode`
`verified-by: bravebot_tui::app::shift_enter_works_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_newline_lands_at_the_caret`
`verified-by: bravebot_tui::app::a_newline_does_not_arm_shell_mode`
`verified-by: bravebot_tui::app::enter_still_sends_a_paragraph_written_with_shift_enter`
`verified-by: bravebot_tui::app::enter_submits_the_prompt`
`verified-by: bravebot_tui::app::enter_on_empty_input_does_nothing`


<a id="INPUT-3"></a>
### INPUT-3: a marker is deletable, and deleting it takes the thing off

This holds for a folded paste, a pasted picture and a dropped file alike. It
is why a marker exists rather than a list the user cannot edit.

The row beneath the box goes with the marker. What is drawn there is what the line in the box
carries, so a file whose marker has been rubbed out is drawn nowhere, and one whose marker is still
there is drawn whether or not a turn is running.

**Why.** The turn was always built from the markers the line still held, so a deleted one already
sent nothing. What lingered was the row, which is the only place a person can see whether rubbing
the marker out worked: left drawn, it says a file is going that is not.

`verified-by: bravebot_tui::drop::deleting_the_marker_takes_the_attachment_off`
`verified-by: bravebot_tui::state::deleting_a_marker_takes_the_picture_back`
`verified-by: bravebot_tui::state::deleting_the_marker_takes_the_paste_back`
`verified-by: bravebot_tui::drop::several_files_dropped_together_each_get_a_marker`
`verified-by: bravebot_tui::drop::sending_a_line_clears_what_was_attached_to_it`
`verified-by: bravebot_tui::render::an_attached_file_is_named_under_the_box`
`verified-by: bravebot_tui::render::deleting_the_marker_takes_the_row_out_from_under_the_box`


<a id="INPUT-4"></a>
### INPUT-4: the keys that stop, and the one that also leaves

Escape discards a half-typed prompt, and does nothing at all on a line with nothing on it. It
never ends the session.

**Ctrl-C stops the nearest thing there is to stop, and leaves when there is nothing left.** It is
read against what is happening, in this order:

| What is happening | What Ctrl-C does |
|---|---|
| a turn in flight, or a command running | stops it, and the session stays where it was |
| nothing running, a line in the box | takes the line, and offers the way out |
| nothing running, an empty box | offers the way out |
| nothing running, an empty box, the way out already offered | ends the session |

**Nothing on this ladder ends the session on one press.** An interrupt is a single byte, and a
terminal delivers one byte stream without saying who wrote it, so a program able to write into the
pty can press this key: the editor that activates a virtualenv writes one before the line it types
(#403), and on the rung that left, that byte ended the session and handed the rest of the line back
to the shell. Every rung above the last one stops something a person asked for and still answers on
the first press, because those are recoverable and leaving is not. Ctrl-D is held to the same rule
for the same reason, being one byte that leaves an empty box.

**And neither half of that gesture is taken from a key that did not arrive on its own.** Asking for a
second press buys nothing against a writer that sends two, and two bytes in one write are no harder
to send than one: `\x03\x03` would otherwise arm the offer and take it. A run is one read of the
terminal, so everything in it was available at the same instant and no part of it is evidence separate
from the rest ([INPUT-34](#INPUT-34) is where a run is defined). A key that arrived with others
therefore reaches the rungs that stop something, which are recoverable, and not the rung that leaves.
This costs a person nothing, since the run a program sent has ended before they press anything.

**The offer is withdrawn by any input that is not one of the two keys that leave**, a mouse report, a
resize and words another program typed among them, not only by another key. It answers the press just
made, and one left standing through ten minutes of scrolling would let a byte written at the end of
them take it.

**What none of this buys: a program can still stop a turn in flight.** The guard is on the rung that
leaves and on no other, and the reason is not that stopping a turn is cheap. It is what the guard would
have to refuse to be worth having.

A key that **starts** something can wait. The return that sends a line and the key that grants a
directory both hand something to the rest of the program that was not there before, and refusing one
costs a person one more press and a line saying which, with the line still in front of them. A key that
**stops** something cannot wait: somebody watching a turn go wrong has to stop it on the first press,
and refusing theirs because the terminal happened to deliver a resize in the same read would take the
interrupt away at the moment it is most wanted. The asymmetry is in what the refusal costs, not in what
the key costs.

It would also buy nothing. The interrupt an editor writes arrives on its own, so a guard asking whether
it arrived alone passes it, and Escape stops a turn on one byte as well. **So an editor writing `\x03`
or `\x1b` into the terminal stops whatever is running, and nothing here prevents it.** What bounds the
damage is that a stopped turn puts its prompt back where the box can take it, so the loss is the tokens
and the time rather than the work. That is a bound and not a defence, and it is written here so nobody
reads the rung that leaves as protecting the rest of the ladder.

Escape only ever stops, and never leaves. A summary is the one exception to the table: it is a
single request with no round for a stop to land between, so nothing there can stop it and Ctrl-C
leaves once it comes back. An aside is such a request too, and so is the check a goal is judged by,
which [goal.md](goal.md) governs. What the table leaves out of all three is a mode: a scroller, a
delegate's view or a prompt search open over the request answers both keys itself, exactly as it
does over a turn, and the press that reaches the request is the one after the mode has closed.

Taking the line says so, on the line beneath the box, and says which key ends the session. The
offer lives for exactly one press, since it answers the press just made and the next press is the
answer to it. **An empty box says the same thing**, in the same words and for a stronger reason: a
press that appeared to do nothing and said nothing reads as an interface that has stopped
responding. Any key that is not itself one of the two that leave withdraws the offer, so a press now
and a byte written later are not the two halves of one gesture.

**Stopping shows a cancelled status, and the prompt comes back when the box can take it.**
The reply stops arriving. When no work followed the prompt, no prompts are queued, and the box is
empty, the prompt returns for editing. The status identifies a deliberate cancellation rather than
a failure or a completed answer.

The prompt stays sent, marked stopped, where any of three things is true: the turn had already
done something that is on the screen, there are prompts waiting behind it, or the box is not empty.
The first two mean there is an order to keep, and a line put back in the box would be out of it.
The third is a box that is taken: what is in it is the line the person is looking at, whether they
typed it during the turn or walked back to it, so the prompt has nowhere to be put back to. Taking
it out of the transcript as well would leave no record of what was asked. Recalling an earlier
prompt is separate: [sessions.md](sessions.md) takes a cancelled prompt out of recall even where
its transcript entry stays.

**Nothing waits out work that is only being waited on.** A reply stops whether or not it has begun
arriving, and whether it is the planner's or a processor's; a running command is killed; a pause
between retries is abandoned; and nothing new is sent once a stop has landed. What still finishes
is a tool call already running, because stopping one part way could leave a file half written.

A request being waited on is walked away from rather than interrupted, since a read in progress
cannot be interrupted. The socket is left to be closed when the far end finishes or the connection
times out, which is sound because reading a reply applies nothing and decides nothing.

**Why.** A stop noticed only between rounds would leave the reply streaming to the end while the
screen said "cancelling…". The key meant to stop the answer would leave the answer running and put
a progress report on the screen about a key press, and the longer the reply the longer somebody
waits for the thing they have already stopped.

**Why.** The press somebody makes while an answer is going wrong in front of them is asking for
the answer to stop, not for the session to end, and answering it by leaving takes the transcript
and everything else with it. Ctrl-C is also how a person leaves a terminal program, which is the
other half: it leaves from an empty box, so both requests have a key.

This is not the arrangement where a key pressed twice means two different things by accident. Each
press has something of its own to answer and the state says which, so no press is one that
silently did another one's job. What makes the ladder safe to walk is that each rung is visible:
the turn stopping is on the screen, and the line going says what the next press will do.

Escape used to leave as well, once the line was empty. That made every press a question of what
was in the box: the key for abandoning a thought ended the session as soon as the thought was
short enough, and pressing it twice in a row meant two different things, the second of which was
the exit. One way out, and it is the one people already reach for.

`verified-by: bravebot_tui::app::escape_clears_a_typed_line_without_quitting`
`verified-by: bravebot_tui::app::escape_on_an_empty_line_does_not_quit`
`verified-by: bravebot_tui::app::escape_twice_clears_and_stays`
`verified-by: bravebot_tui::app::ctrl_c_quits_on_the_second_press`
`verified-by: bravebot_tui::app::one_interrupt_another_program_wrote_does_not_end_the_session`
`verified-by: bravebot_tui::app::any_other_key_withdraws_the_offer_to_leave`
`verified-by: bravebot_tui::app::input_that_is_not_a_key_withdraws_the_offer_to_leave`
`verified-by: bravebot_tui::app::two_interrupts_that_arrived_together_do_not_end_the_session`
`verified-by: bravebot_tui::app::two_end_of_transmissions_that_arrived_together_do_not_end_the_session`
`verified-by: bravebot_tui::app::a_refused_way_out_says_so`
`verified-by: bravebot_tui::app::a_press_on_its_own_after_a_run_still_leaves`
`verified-by: bravebot_tui::app::an_interrupt_still_stops_a_turn_on_the_first_press`
`verified-by: bravebot_tui::app::ctrl_c_stops_a_turn_rather_than_leaving`
`verified-by: bravebot_tui::app::ctrl_c_clears_the_line_before_it_leaves`
`verified-by: bravebot_tui::app::ctrl_c_leaves_once_there_is_nothing_left_to_stop`
`verified-by: bravebot_tui::app::a_taken_line_is_not_claimed_where_the_box_was_empty`
`verified-by: bravebot_tui::app::the_way_out_stops_being_offered_at_the_next_press`
`verified-by: bravebot_tui::render::the_way_out_is_offered_where_the_line_went`
`verified-by: bravebot_tui::app::escape_only_stops_and_ctrl_c_is_read_against_what_is_happening`
`verified-by: bravebot_tui::app::a_single_request_says_it_cannot_be_stopped_and_leaves_on_ctrl_c`
`verified-by: bravebot_aichat::client::a_stopped_stream_stops_before_the_reply_is_over`
`verified-by: bravebot_aichat::client::a_stream_stopped_before_it_starts_reports_nothing`
`verified-by: bravebot_aichat::client::a_stop_does_not_wait_out_the_pause_between_attempts`
`verified-by: bravebot_aichat::client::a_stop_does_not_wait_for_the_model_to_start_writing`
`verified-by: bravebot_aichat::client::a_stop_does_not_wait_for_an_endpoint_that_has_not_answered`
`verified-by: bravebot_tui::state::cancelling_before_anything_happens_still_un_sends_the_prompt`
`verified-by: bravebot_tui::state::a_turn_stopped_over_a_typed_line_keeps_the_line_and_the_prompt`
`verified-by: bravebot_tui::app::a_key_that_would_stop_a_turn_is_answered_during_a_summary`
`verified-by: bravebot_tui::app::escape_stops_the_turn_without_ending_the_session`
`verified-by: bravebot_tui::app::ctrl_g_asks_for_the_editor`


<a id="INPUT-5"></a>
### INPUT-5: where a chord cannot reach the process, the fallback is documented rather than silent

Shift-Enter needs a terminal that reports the modifier (Ghostty, Kitty, WezTerm) or one configured
to send a newline; Ctrl-J is the fallback that always works (INPUT-2). Command-V never reaches the
process and can carry only text, so Ctrl-V is the key for a picture, and which key
carries a picture is said once per session.

**Why.** A chord that silently does nothing reads as a broken feature.

`verified-by: bravebot_tui::app::which_key_carries_a_picture_is_said_once_per_session`
`verified-by: bravebot_tui::app::a_paste_clears_the_hint_that_prompted_it`


<a id="INPUT-6"></a>
### INPUT-6: a marker is deleted whole, in one press

Backspace and Delete each take the whole of the marker the caret covers, and Backspace takes the
whole of one it sits just after, whether it stands for a folded paste, a pasted picture or a
dropped file. A covered marker goes before the character in front of it, because it is the thing
the caret is on. Only a marker the box wrote goes this way: square brackets the user typed are
deleted a character at a time, as everything they typed is.

**Why.** A marker is one thing on the screen and one thing to the person looking at it. Taking a
character off the end leaves text that still reads as an attachment standing over something no
longer attached, and the only way to find that out is to keep pressing.

`verified-by: bravebot_tui::state::one_backspace_takes_the_whole_marker`
`verified-by: bravebot_tui::state::backspace_on_a_covered_marker_takes_the_marker`
`verified-by: bravebot_tui::state::backspace_on_a_marker_at_the_start_of_the_line_takes_the_marker`
`verified-by: bravebot_tui::state::one_backspace_takes_the_whole_folded_paste`
`verified-by: bravebot_tui::state::delete_forward_takes_the_whole_marker`
`verified-by: bravebot_tui::state::text_that_merely_looks_like_a_marker_is_deleted_one_character_at_a_time`
`verified-by: bravebot_tui::drop::one_backspace_takes_the_whole_marker`


<a id="INPUT-7"></a>
### INPUT-7: the caret steps over a marker whole, and never rests inside one

One press of Left or Right crosses a marker in either direction, and there is no position within
one for the caret to stop at. Up and Down keep their place along the line, and where that place
falls inside a marker the caret comes to rest on the marker instead.

**Why.** A caret between two halves of a picture is in a place the person cannot see, and whatever
they type next lands there. Counting out the characters a marker happens to be spelled with is a
dozen presses to cross what reads as a single word.

`verified-by: bravebot_tui::state::the_caret_steps_over_a_marker_whole`
`verified-by: bravebot_tui::state::the_caret_cannot_come_to_rest_inside_a_marker`
`verified-by: bravebot_tui::state::the_caret_cannot_come_to_rest_inside_a_marker_on_another_line`
`verified-by: bravebot_tui::state::typing_after_a_move_between_lines_leaves_the_picture_attached`


<a id="INPUT-8"></a>
### INPUT-8: the caret is drawn over the whole marker it is on

Every cell of the marker is covered, including the part of one the box wrapped onto the next row.

**Why.** The caret says what the next press acts on. A block over the opening bracket alone says
the next press takes a bracket, which is the thing that no longer happens.

`verified-by: bravebot_tui::render::the_caret_covers_a_whole_marker`
`verified-by: bravebot_tui::render::a_marker_the_wrap_split_is_covered_on_both_rows`

<a id="INPUT-9"></a>
### INPUT-9: the box behaves the same whether or not a turn is running

Typing, editing, pasting, dropping a file, putting a line away, walking back through earlier
prompts, scrolling the transcript, asking what a turn has done, choosing how much the session asks
before it acts, and asking what the keys are all do while a turn is in flight exactly what they do at
rest. What a running turn refuses is **sending**, and the keys allowed to mean something else are
named here and nowhere else:

| Key | Why it may differ |
|---|---|
| Enter | sends, which is the whole of what is refused (INPUT-10), and a line that is one of the words a slash may begin waits to be carried out rather than to be sent ([commands.md](commands.md)) |
| Escape, Ctrl-C | stop the turn in flight (INPUT-4) |
| Ctrl-D | leaves, which is not something the box does |
| Ctrl-G | hands the screen the turn is drawing on to an editor (INPUT-14) |
| `!` | arms a mode that changes what Enter does, over whatever the box holds when the turn ends ([shell-mode.md](shell-mode.md)) |
| Up | takes back what is waiting before it walks the history (INPUT-18) |

**What is offered beneath the box is machinery for finishing a line that is about to be sent**, so a
running turn offers nothing to complete. The list of keys (INPUT-13) is not that: it is
documentation somebody asked for, and it is drawn whether or not a turn is running.

**Why.** The box took nothing at all mid-turn once, and it was opened up a piece at a time:
characters, then editing, then pasting. Walking the history was left behind, so a person could
compose a new prompt during a turn but could not reach the one they had just sent, which is the
one they want most when a turn is going wrong in front of them. The keys reached no arm and did
nothing at all, not even the scrolling they fall through to at rest.

A difference between the two has to be a difference about sending, and it has to be in the table.
The two paths therefore answer the same **set** of keys rather than one list each: a test that
walked six key codes said nothing whatever about the keys that were not among them, and three keys
that send nothing were answered by the idle path alone for exactly as long as that. Two of the three
were advertised on every frame of a running turn by the hint line and by the list, so the keys a
person was most likely to reach for while a turn went wrong were the ones that did nothing.

Worst of the three was a key whose flag was set and whose answer was refused a place on the screen.
The press did nothing a person could see, and the list came up when the turn ended, unasked and
attached to no press.

`verified-by: bravebot_tui::app::the_two_paths_answer_the_same_set_of_keys`
`verified-by: bravebot_tui::app::the_way_out_stops_being_offered_at_the_next_press_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_question_mark_lists_the_keys_while_a_turn_runs`
`verified-by: bravebot_tui::app::nothing_is_offered_for_completion_while_a_turn_runs`
`verified-by: bravebot_tui::app::the_trail_can_be_asked_for_while_a_turn_runs`
`verified-by: bravebot_tui::render::a_question_mark_lists_every_shortcut_while_a_turn_runs`
`verified-by: bravebot_tui::app::up_recalls_a_previous_prompt_while_a_turn_is_running`
`verified-by: bravebot_tui::state::recall_works_while_a_turn_is_running`
`verified-by: bravebot_tui::state::a_recalled_prompt_still_cannot_be_sent_while_a_turn_is_running`
`verified-by: bravebot_tui::app::a_long_paste_folds_while_a_turn_is_running`
`verified-by: bravebot_tui::app::a_file_dropped_while_a_turn_is_running_is_attached`
`verified-by: bravebot_tui::app::ctrl_j_is_not_swallowed_while_a_turn_runs`
`verified-by: bravebot_tui::app::ctrl_v_reads_the_clipboard_during_a_turn_too`

<a id="INPUT-10"></a>
### INPUT-10: a prompt sent while a turn runs goes into that turn, at its next round boundary

Enter mid-turn takes the line out of the box and holds it. It is drawn under the box, marked, so
the person can see that what they sent went somewhere.

**A line that is a command is taken the same way, and waits to be carried out rather than to be
sent.** It comes off the box and is drawn under it like anything else waiting, but it is not offered
to the turn in flight, so nothing about it reaches the planner. What carries it out is the queue
being reached once the turn has ended, and a prompt behind it goes when it has, as any waiting prompt
does. Which lines are commands is [commands.md](commands.md)'s.

**The turn in flight takes it.** A turn asks between rounds, after the round's tool calls have run
and before the next request goes out, and everything waiting goes into the conversation there, in
the order it was typed. So an instruction reaches the planner while the work it is about is still
happening. A prompt still waiting when the turn ends becomes a turn of its own, as every queued
prompt used to, and the rest go on waiting under the same rule.

**Why.** A prompt that waits for the turn to end is not an instruction, it is a comment. Somebody
watching an agent read the wrong file and typing "no, the other one" is talking about what is
happening now; delivered after the answer, it arrives after the thing it was meant to prevent, and
the work it would have redirected has been done. This is what Claude Code does, and for this reason.

**The turn in flight is the one on the screen.** Work handed out to a delegate is a turn of its own,
and the person typing may not know one is running at all, so what they send waits for the turn they
are watching rather than going to the delegate. A delegate does not ask for one either: the queue is
shared and asking is taking, so a delegate that asked and then declined what it was handed would
throw the line away, leaving the turn it was aimed at to find nothing waiting. A round a turn spends
waiting for a delegate is a round of that turn, and the boundary it reaches when the delegate is back
asks like any other: a turn that answered with nothing in order to wait and then went straight on to
its next request would put the delegate's report to the planner and nothing of what the person made
of it.

Not mid-round. Every call the planner asked for in a round runs, because a round is a set of calls
asked for together and answering some while abandoning others leaves calls unanswered. Stopping in
the middle of one is what the keys in INPUT-4 are for.

**It may not route.** Routing is precommitted from the prompt that began the turn and stays that
way, so an interjection reaches the planner as words to read and every effect it goes on to ask for
is gated against the routing the turn began with. It is trusted, on the footing of the opening
prompt and by the same act, since a keystroke has no author but the person at the keyboard. The
audit trail records it as the user's own input, so a turn that changed course halfway through does
not read as one that thought of it unprompted. See [routing.md](routing.md).

**What it carries is text.** A turn already running cannot be handed a file or a picture: it fixed
the shape of its context before it read anything. So markers resolve to words when the line is sent,
and the person keeps looking at what they typed: a dropped file resolves to its name, which the
planner can go and read. See [dropping.md](dropping.md).

A waiting prompt is **not** in the transcript. It has not happened; it moves there at the moment the
planner is given it, whether that is inside the running turn or as a turn of its own, and it is
drawn as waiting only until then. What it names is settled when it is queued, not when it is sent,
because a file the person took off the line afterwards was never part of that prompt. It is in the
prompt history from the moment it is queued, since from the person's side that is when they sent it.

Stopping a turn leaves the queue alone. The next waiting prompt begins its turn as it would after
any turn, and the rest go on waiting in order. A prompt is taken back out of the queue by asking
for it (INPUT-18), and until then it goes.

The stopped prompt does **not** come back to the box when something is waiting. It stays in the
transcript as sent, marked stopped, and the box stays empty for whatever the person types next.

**Why.** A stop is aimed at the turn in flight, and nothing else. The prompts behind it are ones
the person typed and has not taken back, so throwing them away made stopping a turn that had gone
wrong cost every prompt they had queued while it went wrong, which is a reason not to press the
key at all.

Un-sending it in front of them would be worse than losing it. The conversation has to read in the
order it happened, and a prompt lifted back out of it while the two typed after it are still
running is in neither place: gone from the transcript, and sitting in a box that is about to be
wanted for the next thing.

An interjection this turn already took keeps it sent too, although taking it left nothing waiting.
It is in the conversation the turn carries on with and it is in the transcript where it was said, so
neither prompt moves: handing the opening one back to the box would leave its own entry above as
sent, and lifting the interjection out to make room would take back a line the planner has read.

Shift-Enter still starts a line rather than sending it, so a paragraph can be written mid-turn and
is not sent half-finished.

**Why.** Enter mid-turn used to reach nothing at all. The line stayed in the box until the person
noticed the turn had ended and pressed it again, which is indistinguishable from a key press that
was ignored. This does not weaken what a running turn refuses: a second turn still cannot begin
while the first is in flight, and the queue is what makes that refusal visible instead of silent.

`verified-by: bravebot_tui::app::enter_queues_a_prompt_while_a_turn_is_running`
`verified-by: bravebot_tui::app::a_command_typed_while_a_turn_runs_is_not_sent_as_a_prompt`
`verified-by: bravebot_tui::app::starting_a_line_mid_turn_does_not_queue_it`
`verified-by: bravebot_tui::app::a_prompt_queued_mid_turn_is_within_the_running_turns_reach`
`verified-by: bravebot_tui::app::a_queued_prompt_joins_the_transcript_when_the_planner_is_given_it`
`verified-by: bravebot_tui::app::a_prompt_that_outlived_the_turn_is_sent_once`
`verified-by: bravebot_tui::app::a_prompt_queued_behind_a_command_is_sent_once_the_command_has_run`
`verified-by: bravebot_tui::app::what_is_still_waiting_stays_in_step_with_what_is_drawn`
`verified-by: bravebot_agent::turn::a_prompt_typed_mid_turn_reaches_the_planner_on_the_next_round`
`verified-by: bravebot_agent::turn::a_prompt_typed_mid_turn_is_recorded_as_the_users_own_input`
`verified-by: bravebot_agent::turn::a_prompt_typed_while_a_delegate_runs_still_reaches_the_turn_that_spawned_it`
`verified-by: bravebot_tui::state::a_prompt_sent_while_a_turn_runs_waits_for_it`
`verified-by: bravebot_tui::state::a_waiting_prompt_goes_when_the_turn_ends`
`verified-by: bravebot_tui::state::waiting_prompts_go_in_the_order_they_were_typed`
`verified-by: bravebot_tui::state::stopping_a_turn_keeps_what_was_waiting_behind_it`
`verified-by: bravebot_tui::state::a_stopped_prompt_stays_sent_where_others_are_waiting`
`verified-by: bravebot_tui::state::a_stopped_prompt_comes_back_where_nothing_is_waiting`
`verified-by: bravebot_tui::state::a_stopped_turn_that_took_an_interjection_leaves_both_prompts_where_they_are`
`verified-by: bravebot_tui::app::stopping_a_turn_that_took_a_prompt_mid_turn_hands_nothing_back`
`verified-by: bravebot_tui::state::a_waiting_prompt_is_in_the_history_already`
`verified-by: bravebot_tui::state::there_is_nothing_to_queue_when_the_line_is_blank_or_nothing_is_running`
`verified-by: bravebot_tui::render::a_waiting_prompt_is_shown_as_waiting`
`verified-by: bravebot_tui::render::a_prompt_stops_waiting_once_its_turn_begins`
`verified-by: bravebot_tui::render::a_prompt_stops_waiting_once_the_running_turn_takes_it`

<a id="INPUT-11"></a>
### INPUT-11: what is attached is drawn nearest the box, above what is waiting

The rows beneath the box run in one order: what the line in the box carries, then the prompts
waiting for the turn in flight, then what the half-typed line could still become.

**Why.** An attachment is part of the line still being composed, and the prompts below it have
already gone. Drawn the other way round, a file staged during a turn sat underneath prompts it
was no part of, which reads as though it went with one of them, and the row for the file the
person had just dropped moved further from the box with every prompt they queued.

`verified-by: bravebot_tui::render::what_is_attached_is_drawn_above_what_is_waiting`
`verified-by: bravebot_tui::render::an_attached_file_is_named_under_the_box`
`verified-by: bravebot_tui::render::a_waiting_prompt_is_shown_as_waiting`

<a id="INPUT-12"></a>
### INPUT-12: an empty box says what it is for

An invitation stands in the empty box, behind the same prompt character a typed line gets and in
the column the first character will land in, with the caret on it. The first thing typed takes its
place and nothing on the row moves. It is drawn rather than typed, so it is never part of a prompt
and never has to be deleted. Shell mode has none: the line there is a command, and its own prompt
character, colour and hint line all say so.

**Why.** An empty box says nothing about what it takes, and the one thing somebody opening this
for the first time needs to know is that they may simply ask.

`verified-by: bravebot_tui::render::an_empty_box_says_what_it_is_for`
`verified-by: bravebot_tui::render::the_invitation_stands_where_the_first_character_will`
`verified-by: bravebot_tui::render::the_invitation_goes_the_moment_anything_is_typed`
`verified-by: bravebot_tui::render::the_invitation_comes_back_when_the_line_does_not`
`verified-by: bravebot_tui::render::the_invitation_is_not_offered_where_the_line_is_a_command`

<a id="INPUT-13"></a>
### INPUT-13: `?` on an empty line lists every key, and the hint line says only that

The marker is a mode rather than a character, as `!` is (INPUT-2, [shell-mode.md](shell-mode.md)):
nothing is typed into the box, the invitation stays where it was, and there is nothing to delete
afterwards. A second `?` takes the list down, as does Escape or typing anything else. Only on an
empty line, since a `?` in a sentence is the punctuation somebody is asking a question with, and in
shell mode it is a glob for the shell to expand.

**A line arriving in the box takes the list down**, whichever path put it there: recall, a stashed
line coming back, the queue coming back, an editor, or a stopped turn handing its prompt back. The
list stands over the box, so one left up over a line that arrived under it belongs to a press two
prompts ago.

The list is not a completion. There is nothing in it to choose, so Tab and the arrows go on meaning
what they mean everywhere else while it is up.

It is the one place the keys are written down, so a binding that changes cannot leave the list
advertising something that no longer works. It folds into as many columns as the width holds, and no
row runs past the edge: a row that wrapped would put the list a row over the height reserved for it
and push the hint line off the screen.

The hint line carries what the session is doing (the mode in force where it is not asking, the
trail, how full the context is and on what footing it knows that, INPUT-22, a loop that is running
and when its next tick is due, [loop.md](loop.md), the key that opens the delegates where the
session has spawned any) and then `? for shortcuts`. It lists no other binding of its own, and it does not report the
confinement. The trail key is named only **once a turn has left a trail to look at**: a trail is
recorded when the turn it belongs to ends, so before then the line would be offering a press that
changes nothing on screen.

The mode leads the line and is the only part of it drawn in a colour, and asking takes no room at
all: what is drawn is a mode somebody chose, named in [permission-modes.md](permission-modes.md). A
marker standing there on every session is one people stop reading, and being read is the whole of
what this one is for.

**What does not fit is dropped whole, at a separator.** The parts are given up in order (a reading
with no figure in it, then the way to the bindings, then the trail key, then the figures, and a
running loop after all of them), and the mode is the last to go. The loop is kept that late because
it is the only part of the line spending something while nobody is watching, and it is given up at
all because a part nothing may give up would have a narrow terminal clear the whole row and take the
mode with it. In shell mode the line is the shell's own, and the loop is said there too, since a loop
spends a turn whichever mode the box is in. A note about what a press just did, drawn at the right of
the same row,
takes its room ahead of all of them: the parts are fitted against the width it leaves, since a part
fitted against the whole width is one the note writes over the middle of. Left to the
terminal, the line is cut wherever the final column falls, which puts half a word under the box:
that reads as a rendering fault, where a part that is simply absent reads as a line with no room,
which is the truth.

**Why.** The bindings and the state were on one line together, and the line was wider than the
terminal, so the end of it was cut. Everything a person could look up was taking room from the two
figures they had no other way to see. A binding cut off is one somebody learns once; a context
reading cut off is gone. Moving the bindings behind a key they can press when they want them is what
lets the line fit a terminal eighty wide whole.

The confinement is settled by the platform before the session opens and cannot change while it
runs, so a line reporting it on every frame spends room on a constant. The mark states it once at
startup and `/status` answers for it whenever somebody asks. The delegate key earns the room
instead, because it is the one thing here whose answer changes and which nothing else on a
finished screen says.

The mode outranks all of it because it is the one thing here that changes what the next keystroke
does. A person who cannot see that writes are going through unasked is the case this line exists to
prevent, and a mode read off `/status` after the write is a mode read too late.

`verified-by: bravebot_tui::app::a_question_mark_on_an_empty_line_toggles_the_list_without_being_typed`
`verified-by: bravebot_tui::app::a_question_mark_inside_a_sentence_is_punctuation`
`verified-by: bravebot_tui::app::a_question_mark_in_shell_mode_is_a_glob`
`verified-by: bravebot_tui::app::typing_takes_the_list_down`
`verified-by: bravebot_tui::app::escape_takes_the_list_down`
`verified-by: bravebot_tui::state::a_line_that_arrives_under_the_list_takes_the_list_down`
`verified-by: bravebot_tui::render::a_question_mark_lists_every_shortcut`
`verified-by: bravebot_tui::render::the_list_names_the_chord_that_opens_the_scroller`
`verified-by: bravebot_tui::render::the_shortcuts_are_not_something_to_complete`
`verified-by: bravebot_tui::render::the_shortcuts_use_fewer_rows_where_the_width_allows`
`verified-by: bravebot_tui::render::no_shortcut_row_runs_past_the_edge`
`verified-by: bravebot_tui::render::the_hint_line_says_how_to_find_the_bindings`
`verified-by: bravebot_tui::render::the_hint_line_does_not_report_the_confinement`
`verified-by: bravebot_tui::render::the_hint_line_names_the_delegate_key_once_one_has_run`
`verified-by: bravebot_tui::render::the_hint_names_the_trail_key_only_once_there_is_a_trail`
`verified-by: bravebot_tui::render::the_hint_line_fits_a_narrow_terminal_whole`
`verified-by: bravebot_tui::render::the_hint_and_the_list_name_the_same_key`
`verified-by: bravebot_tui::render::the_hint_line_names_a_mode_that_is_not_asking`
`verified-by: bravebot_tui::render::the_hint_line_says_nothing_about_the_ordinary_mode`
`verified-by: bravebot_tui::render::the_hint_line_says_a_loop_is_live`
`verified-by: bravebot_tui::render::the_hint_line_says_a_loop_is_live_in_shell_mode_too`
`verified-by: bravebot_tui::render::the_hint_line_says_nothing_about_a_loop_in_a_session_with_none`
`verified-by: bravebot_tui::render::a_narrow_terminal_gives_up_the_bindings_rather_than_the_mode`
`verified-by: bravebot_tui::render::a_narrow_terminal_gives_up_a_reading_before_the_loop_and_the_loop_before_the_mode`
`verified-by: bravebot_tui::render::what_does_not_fit_is_dropped_whole_rather_than_cut_mid_word`
`verified-by: bravebot_tui::render::a_reading_with_no_figure_in_it_is_given_up_before_a_binding`
`verified-by: bravebot_tui::render::a_note_at_the_right_takes_its_room_from_the_parts_rather_than_over_them`
`verified-by: bravebot_tui::shell_mode::the_shortcuts_offer_shell_mode`

<a id="INPUT-14"></a>
### INPUT-14: Ctrl-G edits the line in the user's own editor, and only what was saved comes back

The editor opens on what has been typed so far, so the key continues a prompt rather than starting
it again, and what was saved replaces the line. Quitting without saving leaves the line exactly as
it was, and so does an editor that failed or was killed: neither says anything about what the user
wanted. The trailing newline an editor leaves is dropped, one only, and line endings come back the
way a paste's do. The file the editor opened is the user's own words, is readable by nobody else,
and does not outlive the edit down any path. The key does nothing while a turn runs.

**Why.** A prompt worth thinking about outgrows a box ten rows tall with nothing to search or
reflow with. The failure that matters is the one that blanks a paragraph somebody just wrote, so
every path that does not end in a save ends in the line untouched. Handing the terminal to an
editor mid-turn would take the screen from the turn drawing on it.

`verified-by: bravebot_tui::app::ctrl_g_asks_for_the_editor`
`verified-by: bravebot_tui::app::the_editor_key_does_nothing_while_a_turn_runs`
`verified-by: bravebot_tui::state::a_line_from_the_editor_replaces_what_was_typed`
`verified-by: bravebot_tui::editor::the_editor_opens_on_what_was_already_typed`
`verified-by: bravebot_tui::editor::what_the_editor_saved_becomes_the_line`
`verified-by: bravebot_tui::editor::quitting_without_saving_leaves_the_line_as_it_was`
`verified-by: bravebot_tui::editor::an_editor_that_failed_does_not_produce_a_line`
`verified-by: bravebot_tui::editor::the_newline_an_editor_leaves_at_the_end_is_dropped`
`verified-by: bravebot_tui::editor::only_the_last_newline_goes`
`verified-by: bravebot_tui::editor::line_endings_come_back_the_way_a_paste_does`
`verified-by: bravebot_tui::editor::the_file_does_not_outlive_the_edit`

<a id="INPUT-15"></a>
### INPUT-15: `$VISUAL`, then `$EDITOR`, then a list that prefers a full editor to a last resort

An empty value is not an answer, since exporting a variable to nothing is how a profile takes one
back. With neither set, what opens is the first of `vim`, `vi`, `emacs`, `nano` that is installed,
in that order. A configured editor that will not start is reported as such and nothing else is
tried. An editor that returns before the file has been edited is told to wait, but only where the
user wrote no arguments of their own.

**Why.** Someone with `vim` or `emacs` on their machine chose to install it and will not thank a
guess for opening something else; `nano` is the last resort, for the person who has none of them.
Falling back past a name the user exported would run an editor they did not ask for and blame their
configuration for it. A GUI editor that exits the moment its window opens returns the line
unchanged with nothing anywhere saying why, which is neither a failure nor an edit.

`verified-by: bravebot_tui::editor::visual_answers_before_editor`
`verified-by: bravebot_tui::editor::an_empty_variable_is_not_a_configured_editor`
`verified-by: bravebot_tui::editor::a_full_editor_is_preferred_to_the_last_resort`
`verified-by: bravebot_tui::editor::the_fallback_list_is_the_same_on_every_platform`
`verified-by: bravebot_tui::editor::a_configured_editor_that_will_not_start_ends_the_search`
`verified-by: bravebot_tui::editor::a_gui_editor_is_told_to_wait`
`verified-by: bravebot_tui::editor::the_flag_follows_the_program_through_a_path`
`verified-by: bravebot_tui::editor::a_terminal_editor_is_given_no_extra_flag`

<a id="INPUT-16"></a>
### INPUT-16: an editor is started under the name it was asked for

A name is looked for where a shell would look for it, and what runs is that name and not the file a
symlink behind it points at. The name is only kept while it still reaches the same program: a link
that now points elsewhere, or one that no longer resolves, is started by resolved path instead. This
is the editor alone. Everywhere a program is approved before it runs, the approval names the file
that ran, because a name can be repointed afterwards.

**Why.** MacVim installs `vim`, `vi` and `gvim` as links to a single shim that reads its own
`argv[0]`, stays in the terminal for the `vi*` spellings and forks a detached GUI window for the
`m*` and `g*` ones. Started through the resolved path it is always `mvim`, so asking for `vim` opened
a window, returned at once, and put the prompt back unedited with nothing saying why. An editor is
started rather than approved, and for it the name is part of what the user asked for.

`verified-by: bravebot_tui::editor::a_configured_link_to_a_gui_shim_runs_as_the_link`
`verified-by: bravebot_tui::editor::a_terminal_editor_behind_a_gui_shim_is_started_under_its_own_name`
`verified-by: bravebot_tui::editor::a_program_reached_through_a_link_keeps_the_name_it_was_asked_for`
`verified-by: bravebot_tui::editor::the_name_is_looked_for_on_the_path_not_beside_the_resolved_file`
`verified-by: bravebot_tui::editor::an_empty_path_entry_is_not_searched`
`verified-by: bravebot_tui::editor::a_name_that_is_no_longer_the_same_program_falls_back_to_the_resolved_path`
`verified-by: bravebot_tui::editor::a_name_the_path_holds_under_another_spelling_still_starts_the_program`
`verified-by: bravebot_tui::editor::a_name_is_looked_for_under_every_spelling_it_may_be_filed_under`

<a id="INPUT-17"></a>
### INPUT-17: Ctrl-S puts the line away, and puts it back

One key, read against the line rather than remembered. A line in the box is put away and the box is
emptied; an empty box is where a line put away earlier comes back, with the caret at its end, where
somebody carries on typing. There is one place to put a line, so a second line put away replaces the
first, and a line that comes back is no longer there to come back again: the next press on the empty
box it left has nothing to do, and says nothing. A prompt walked back to is the one line the key does
not put away, and means the search there instead (INPUT-31).

**The words travel and the mode does not.** What is put away is what the user typed, and `!` is a
mode rather than a character (INPUT-2, [shell-mode.md](shell-mode.md)), so it stays where they left
it. A prompt comes back into an armed shell as the command they are writing now, and a command comes
back onto an ordinary prompt as words. The list of keys goes, as it does for anything else that
rewrites the box.

**Nothing is sent, so a turn in flight refuses none of it** (INPUT-9). What a line names is settled
when it is sent and not when it is put away, so a marker in a stashed line stands for something still
staged, and names it again when the words holding it come back.

**A line put away says so, on one row beneath the box, and says which key returns it.** One row
however long the line was, above the prompts that are waiting and below what the line in the box
carries (INPUT-11). No row runs past the edge, and where the width will not hold both, the words stay
and the reminder goes: which line is waiting is the part only this row can say, and the key is in the
list `?` puts up as well.

**Why.** A better thought arrives while a worse one is half written, most often during a turn, and
the two ways out were sending the first or losing it. Escape is not a third: it discards, and a
person who wanted the words back has nowhere to have got them from.

The row is the whole of what makes the key safe to press. A press that emptied the box and said
nothing is indistinguishable from one that threw a paragraph away, and the only way to find out
which it had been was to press again and hope. Naming the line is what turns the key from a guess
into a place a thought is being kept.

The caret is not carried. It belongs to an edit that has finished, and restoring it would put a
person back in the middle of a sentence they have not looked at since.

`verified-by: bravebot_tui::app::ctrl_s_puts_the_line_away_and_brings_it_back`
`verified-by: bravebot_tui::app::the_stash_key_works_while_a_turn_runs`
`verified-by: bravebot_tui::state::a_stashed_line_comes_back_as_it_was`
`verified-by: bravebot_tui::state::the_caret_lands_at_the_end_of_a_line_brought_back`
`verified-by: bravebot_tui::state::stashing_again_replaces_what_was_put_away`
`verified-by: bravebot_tui::state::a_line_brought_back_cannot_be_brought_back_again`
`verified-by: bravebot_tui::state::stashing_an_empty_line_with_nothing_put_away_does_nothing`
`verified-by: bravebot_tui::state::the_mode_is_not_stashed_with_the_line`
`verified-by: bravebot_tui::state::a_command_comes_back_as_words_and_not_as_a_command`
`verified-by: bravebot_tui::state::a_line_can_be_stashed_while_a_turn_runs`
`verified-by: bravebot_tui::state::what_a_stashed_line_named_is_still_named_when_it_comes_back`
`verified-by: bravebot_tui::render::a_stashed_line_is_named_under_the_box`
`verified-by: bravebot_tui::render::the_row_goes_when_the_stashed_line_comes_back`
`verified-by: bravebot_tui::render::a_stashed_paragraph_is_one_row`
`verified-by: bravebot_tui::render::what_is_stashed_is_drawn_between_the_attachments_and_the_queue`
`verified-by: bravebot_tui::render::no_stashed_row_runs_past_the_edge`
`verified-by: bravebot_tui::render::a_narrow_terminal_keeps_the_words_and_drops_the_reminder`

<a id="INPUT-18"></a>
### INPUT-18: Up takes back what is waiting before it walks the history

While prompts are waiting, Up puts all of them back into the box in one press, in the order they
were typed, one to a line. Nothing is waiting afterwards, so the rows under the box that said so
go with them. A half-typed line stays below them, where the caret is. What each of them named
comes back staged with it, so a marker in a line that comes back stands for the same file or
picture it stood for when it went. They stay in the prompt history, since from the person's side
they were sent and taking them back does not unsay them.

**Only what the planner has not been given.** A prompt the running turn has already taken
(INPUT-10) cannot be taken back, because it is in the conversation: offering it to the box would
leave the person editing a line that had gone, and sending it again would say it twice. Taking one
back that the turn has *not* reached puts it out of the turn's reach as well, or the key would read
as having done nothing: the line in the box, and the copy arriving at the planner a moment later.
Where the turn has taken every waiting prompt, the press leaves the box exactly as it was.

With nothing waiting the key is unchanged: it walks the history, and scrolls once there is nothing
left to walk. Inside a paragraph it moves between rows first, and reaches the queue from the top
row, the way it reaches the history there.

**Why.** Up is how a person reaches for the last thing they said, and while something is waiting
the last thing they said is in the queue. The history holds a copy of every queued line from the
moment it is queued (INPUT-10), so the key handed back a copy: the person rewrote it, sent it, and
the original went as well. The only way to take a queued prompt back was to stop the turn in
flight, which is aimed at something else entirely and costs the answer being written.

`verified-by: bravebot_tui::app::up_takes_back_everything_waiting_rather_than_a_copy_of_it`
`verified-by: bravebot_tui::app::up_walks_the_history_again_once_nothing_is_waiting`
`verified-by: bravebot_tui::app::taking_the_queue_back_takes_it_out_of_the_turns_reach`
`verified-by: bravebot_tui::app::a_prompt_the_turn_has_taken_cannot_be_taken_back`
`verified-by: bravebot_tui::state::taking_the_queue_back_puts_every_waiting_prompt_in_the_box`
`verified-by: bravebot_tui::state::a_half_typed_line_stays_below_what_comes_back`
`verified-by: bravebot_tui::state::what_a_waiting_prompt_named_is_named_again_when_it_comes_back`
`verified-by: bravebot_tui::state::there_is_nothing_to_take_back_when_nothing_is_waiting`
`verified-by: bravebot_tui::render::the_waiting_rows_go_when_the_queue_is_taken_back`

## Known costs

- **A stopped request leaves a thread and a socket behind.** The reply goes on being read by
  nobody until the far end finishes or the connection times out, which can be minutes on a
  connection that has died. It costs a thread and a socket for that long, and it is the price of
  answering the person at once: the alternative is waiting for a read that cannot be interrupted.
  Nothing that thread does is visible, since it holds no policy, no workspace and no tool.
- **Asking the terminal about itself at startup can eat what was typed into that moment.** The
  question about the background is asked once, before the first frame, and the answer is read off
  the tty directly. Anything typed or pasted in the window before the answer arrives is read by
  that same call and discarded, and a terminal that answers with nothing keeps the window open for
  its full 80 ms. It is bounded, it is once per session, and it is before there is a box to type
  into, which is why it is a cost and not a clause.
- **Ctrl-S is the byte a terminal traditionally freezes its output with.** It reaches this process
  because raw mode turns that flow control off for as long as the session holds the terminal, which
  is what makes the chord bindable at all (INPUT-17). The cost is a person's muscle memory: somewhere
  behind a `tmux` or `screen` configured to keep flow control, or an ssh session that does, the key
  can be taken before it arrives, and then it does nothing here. Nothing is lost when that happens,
  since the line stays in the box.
- **A prompt walked back to is the one line that cannot be put away.** The key opens the search there
  instead (INPUT-31), so parking a recalled prompt while something else is typed is a thing the stash
  slot no longer does. What it buys is the search being one key from the walk, which is what makes the
  prompt cheap to reach again: a word typed into the search rather than the walk over again. Touching
  the line is the way to the old meaning, an edit being what ends the walk (INPUT-17).
- **A selection is one stretch of the line and never a column of it.** Vi's block-wise selection,
  which reaches the same columns of several rows, has no equivalent here: what `v` and `V` mark out
  runs from one position to another (INPUT-30). The cost is a person's muscle memory for one chord,
  and it buys a selection that is a pair of offsets rather than a rectangle every operator would have
  to understand separately. A box ten rows tall holding one prompt is also not where somebody edits
  columns of a table.
- **A panic leaves the terminal taken.** Every path that returns hands it back (INPUT-33), and a
  panic returns through none of them: the process ends on the alternate screen, in raw mode, with
  mouse reporting on, and the shell that started it is left needing `reset`. The message that says
  what went wrong is on the screen thrown away with it, which is the worse half. A process-wide
  hook is not the answer on its own: a turn runs off the main thread, and a panic there is a turn
  that failed rather than a session that ended, so a hook that handed the terminal back would do it
  underneath an interface still drawing on it.
- **This interface owns the whole terminal while it runs.** The transcript is a viewport repainted
  in place on a screen of its own rather than lines added to the terminal's scrollback (INPUT-33),
  so what leaves the top of it is reachable through this program's own scroller and through nothing
  else, and a screen repainted in place is not a document a screen reader can follow. What that
  costs is a mode rather than the program: `--plain` is the same session in lines
  ([cli.md](cli.md)), and what it gives up is everything this interface draws, the scroller and its
  search, the key list, the slash commands, `@` naming a file, a picture on the clipboard, and a
  session record to pick up again. So the cost is having to choose, and neither half is the whole
  program.
- **Vi's editing is what this box does with the keys, not what vi does with a file.** There is one
  register rather than named ones, undo is a single step (INPUT-28), counts do not prefix a command,
  and there is no `:` line. Each of those is machinery for a file being edited over an afternoon,
  where this is a prompt being written over a minute, and every one of them is a key that does nothing
  rather than one that does something unexpected.

<a id="INPUT-19"></a>
### INPUT-19: Ctrl-R searches every prompt sent, and what is chosen goes into the box

Ctrl-R opens a search over the prompt history, at rest and while a turn is running, and closes it
again along with Escape and Ctrl-C. It opens on the newest prompt, seeded with whatever single line
the person typed into the box, and with nothing where that line is a prompt walked back to: the
history put that one there whole, and a search looking for it answers with it alone. While it is open
every letter narrows the list rather than reaching the box, and each word typed has to appear
somewhere in a prompt for it to be offered; the arrows walk the matches, Ctrl-S swaps between every
prompt and the ones sent from this workspace, and backspacing past the start closes the search as the
scroller's does. The chord is on the key list (INPUT-13), and in the border of the box while an older
prompt is being walked back to, which is the moment somebody has shown they want one, except where
that border is too narrow to hold it beside which prompt is being shown (INPUT-31).

Enter puts the prompt under the cursor into the box. It does not send it, and it replaces what was
in the box, that line being what the search was seeded with or the prompt the walk put there.

**Why.** Up walks one prompt at a time, which is the right way in when the wanted prompt is the last
one and no way in at all when it is the hundredth: what a person remembers of an old prompt is a
word out of the middle of it rather than how far back it was. Ctrl-R because every shell answers
that chord with this question.

A stored prompt is content, not an instruction: the file can be edited, on a shared machine by
somebody else. Landing it in the box rather than in a request means the keystroke that sends it is
the person's own, after they have read it, exactly as if they had typed it
([sessions.md](sessions.md)).

Mid-turn is when the wanted prompt is most likely to be one that has scrolled away, and searching
sends nothing, which is the whole of what a running turn refuses (INPUT-9). Escape and Ctrl-C reach
the search before they reach the turn, so the key that closes it leaves the turn running and the
press that stops the turn is the next one. A turn is not the only thing they reach it before: the
summary, the aside and the goal check each hold a request of their own, and the search is open over
those the same way.

`verified-by: bravebot_tui::app::ctrl_r_searches_the_prompts_already_sent`
`verified-by: bravebot_tui::app::the_search_starts_from_what_was_already_typed`
`verified-by: bravebot_tui::app::the_search_does_not_start_from_a_prompt_walked_back_to`
`verified-by: bravebot_tui::render::how_to_search_the_prompts_is_said_where_somebody_would_look`
`verified-by: bravebot_tui::app::the_prompts_can_be_searched_while_a_turn_is_running`
`verified-by: bravebot_tui::app::the_search_answers_the_stop_keys_before_the_turn_does`
`verified-by: bravebot_tui::app::the_search_answers_the_stop_keys_before_a_single_request_does`
`verified-by: bravebot_tui::app::the_search_answers_the_stop_keys_before_the_goal_check_does`
`verified-by: bravebot_tui::app::ctrl_r_with_nothing_sent_yet_opens_nothing`
`verified-by: bravebot_tui::app::the_search_starts_from_what_was_already_typed`
`verified-by: bravebot_tui::app::a_letter_narrows_the_search_rather_than_reaching_the_box`
`verified-by: bravebot_tui::app::enter_puts_the_chosen_prompt_in_the_box_without_sending_it`
`verified-by: bravebot_tui::app::escape_closes_the_search_and_leaves_the_box_as_it_was`
`verified-by: bravebot_tui::app::backspacing_past_the_start_of_the_search_closes_it`
`verified-by: bravebot_tui::app::the_arrows_walk_the_matches_while_the_search_is_open`
`verified-by: bravebot_tui::history_search::typing_narrows_the_list_to_the_prompts_that_match`
`verified-by: bravebot_tui::history_search::every_word_typed_has_to_match`
`verified-by: bravebot_tui::history_search::narrowing_puts_the_cursor_on_the_newest_match`
`verified-by: bravebot_tui::history_search::the_search_opens_on_the_newest_prompt`
`verified-by: bravebot_tui::history_search::the_cursor_stops_at_the_oldest_and_at_the_newest`
`verified-by: bravebot_tui::history_search::backspacing_widens_the_list`
`verified-by: bravebot_tui::history_search::the_scope_narrows_to_the_prompts_sent_from_this_workspace`
`verified-by: bravebot_tui::history_search::a_prompt_from_before_workspaces_were_kept_is_in_the_wide_list_only`

<a id="INPUT-20"></a>
### INPUT-20: the list says which prompt each row is

The search is drawn over the transcript in place of the box, newest at the bottom. Each row carries
how long ago that prompt was sent, and nothing where the entry predates times being kept. The
prompt under the cursor is drawn in full beside the list, with a word for how many lines did not
fit; where the terminal is too narrow for two columns the list is what is kept. A window over more
matches than fit says so, the scope in force is named, and a search matching nothing says that
rather than showing an empty panel.

**Why.** A row is one line of what may be a paragraph, and two prompts about the same thing begin
alike: what tells them apart is the rest of the text and when each was sent. An age is the one thing
about an old prompt everybody is sure of, and it is compact here rather than in the words the
session list uses, since it sits in a gutter beside every row rather than in a sentence.

Empty panels are the failure mode of every mode: nothing on screen and no word for it reads as an
interface that has stopped responding rather than as a search that is too narrow.

`verified-by: bravebot_tui::render::the_prompt_search_is_drawn_over_the_transcript_rather_than_instead_of_it`
`verified-by: bravebot_tui::history_search::an_age_is_drawn_beside_every_prompt_that_has_one`
`verified-by: bravebot_tui::history_search::a_prompt_with_no_age_is_drawn_without_one`
`verified-by: bravebot_tui::history_search::the_prompt_under_the_cursor_is_drawn_in_full`
`verified-by: bravebot_tui::history_search::a_prompt_too_long_for_the_panel_says_how_much_is_left`
`verified-by: bravebot_tui::history_search::a_narrow_panel_keeps_the_list_and_drops_the_full_prompt`
`verified-by: bravebot_tui::history_search::a_list_taller_than_the_panel_says_there_is_more_above`
`verified-by: bravebot_tui::history_search::a_search_matching_nothing_says_so`
`verified-by: bravebot_tui::history_search::the_scope_is_named_on_the_panel`

<a id="INPUT-21"></a>
### INPUT-21: Shift-Tab chooses how much the session asks before it acts

The key walks the modes in order and comes back round to the first, so no mode is one a person
cannot press their way out of. Which modes there are, and what each answers, is
[permission-modes.md](permission-modes.md); the mode in force is drawn on the hint line (INPUT-13).

Both spellings of the chord are answered. A terminal asked to disambiguate reports Shift-Tab as Tab
with a modifier, and one that has not sends the older `BackTab`, and which arrives is the terminal's
choice rather than the user's.

The key is read before Tab, so it never completes a half-typed line, and it types nothing: like `!`
and `?` it is a mode rather than a character. It works while a turn runs, which is when it is wanted
most, and the turn in flight keeps the mode it began with.

**Why.** A person watching a turn edit files it should not be editing is deciding about the next
turn, and this is how they say so without stopping the one in front of them. A binding answering one
spelling works on one machine and does nothing on the next, which reads as a broken key rather than
as a terminal difference.

Shift-Tab because it is the chord Claude Code uses for this, and somebody who has used one of these
reaches for it before reading anything.

`verified-by: bravebot_tui::app::shift_tab_cycles_the_permission_mode`
`verified-by: bravebot_tui::app::either_spelling_of_shift_tab_cycles_the_mode`
`verified-by: bravebot_tui::app::the_mode_key_leaves_the_line_alone`
`verified-by: bravebot_tui::app::the_mode_can_be_changed_while_a_turn_runs`
`verified-by: bravebot_agent::permission_mode::the_key_cycles_three_modes_without_the_flag`
`verified-by: bravebot_agent::permission_mode::a_session_started_in_bypass_can_cycle_out_of_it`


<a id="INPUT-22"></a>
### INPUT-22: the context reading says which of three things the session knows

A session that has measured a request states how full the context is, as a percentage of the budget
the conversation is compacted at, capped at a hundred. A conversation shortened underneath that
figure says it was compacted, and how much room that won back, rather than how full the context
is, because the number it held describes an exchange that is not the one on screen. A session that
has measured nothing says that it has not measured anything.

**No state of the session is drawn as a blank.** A count that arrived with no budget to divide it
by states no percentage, and the session then knows no more about how full the context is than one
that has measured nothing, so it reads the same. A reading absent from the line is the width rule
of INPUT-13 dropping it whole, or the line belonging to the shell (INPUT-2), and neither is
something the session knows about the context.

**A compaction states how much room it won back, as a fraction of the budget it was compacted
at.** The summariser read the part of the exchange that stopped being sent and wrote what replaced
it, so the difference between those two counts is the room the conversation gave back. The figure
is marked approximate in every case, and it only ever over-states: the count of what the summariser
read holds the instructions it was given as well as the exchange, and nothing here can take them
off again. A compaction the server reported no usage for says that the conversation was compacted
and states no figure. A budget adopted after a compaction leaves the figure alone, since what was
won back is a fact about the exchange that was shortened rather than about the window in force
now.

**A percentage against a budget nobody advertised is marked as approximate.** The budget is a
window the endpoint reported for the model in force, a figure somebody set by hand, or a default
standing in for both. The default is a number chosen to be safe against models it knows nothing
about, so a reading against it is drawn with a mark saying it is one.

**A measurement is taken wherever one exists, not only where a turn ended well.** A session resumed
from disk opens with what the last request of the session it read came to. A turn that failed after
sending a request reports what that request came to. A turn that sent nothing leaves the reading
where it was, which for a session that has sent nothing at all is absent.

**Why.** This reading is what a person uses to decide whether to compact, and it is the only
account of the size of a conversation that exists here: the server reports what a request cost and
never what it had room for, and there is no tokeniser to count with. Having just compacted is the
moment somebody most wants a figure and the one moment nothing has counted the conversation, since
the shortened one is not counted until the next request goes out; the compaction is itself a
request, so its own reply is where the figure comes from. A state drawn as a blank is
indistinguishable from the other state drawn as a blank, from a line too narrow to hold the figure,
and from a reading that has stopped working, so it makes the figure look intermittent, and a figure
that comes and goes is one people stop reading.

The mark on a guessed budget is the difference between two readings of a hundred per cent that ask
for opposite things. Against a window the endpoint stated, it means shorten the conversation.
Against the default, it may only mean the default is too small for the model in force, and the
answer is to set the budget rather than to compact.

`verified-by: bravebot_tui::state::how_full_the_context_is_comes_back_as_a_percentage`
`verified-by: bravebot_tui::state::a_request_past_the_budget_reads_as_full_rather_than_more_than_full`
`verified-by: bravebot_tui::state::a_context_measured_at_nothing_is_a_context_nobody_has_measured`
`verified-by: bravebot_tui::state::a_compacted_session_reports_what_the_compaction_won_back`
`verified-by: bravebot_tui::state::a_compaction_that_won_no_room_back_states_no_figure`
`verified-by: bravebot_tui::state::room_won_back_with_no_budget_to_state_it_against_is_no_figure`
`verified-by: bravebot_tui::state::a_budget_adopted_after_a_compaction_leaves_what_it_won_back_alone`
`verified-by: bravebot_tui::app::the_room_a_compaction_won_back_is_what_it_read_less_what_it_wrote`
`verified-by: bravebot_tui::state::updating_budget_retains_token_count_with_new_capacity`
`verified-by: bravebot_tui::state::a_budget_that_did_not_move_can_still_stop_being_one_anybody_advertised`
`verified-by: bravebot_tui::state::clearing_a_session_forgets_how_full_the_old_one_was`
`verified-by: bravebot_tui::render::the_hint_line_says_how_full_the_context_is`
`verified-by: bravebot_tui::render::the_hint_line_marks_a_guessed_budget`
`verified-by: bravebot_tui::render::the_hint_line_reports_a_compacted_context`
`verified-by: bravebot_tui::render::the_hint_line_says_how_much_room_a_compaction_won_back`
`verified-by: bravebot_tui::render::a_compaction_with_no_figure_to_give_still_says_the_conversation_was_compacted`
`verified-by: bravebot_tui::render::the_hint_line_says_an_unmeasured_context_has_not_been_measured`
`verified-by: bravebot_tui::render::a_measurement_with_no_budget_to_state_it_against_reads_as_unmeasured`
`verified-by: bravebot_tui::app::a_failed_turn_measures_context_if_requests_were_sent`
`verified-by: bravebot_tui::app::a_failed_turn_with_no_requests_sent_remains_unmeasured`
`verified-by: bravebot_agent::conversation::a_restored_conversation_remembers_what_its_last_request_came_to`
`verified-by: bravebot_agent::conversation::compacting_forgets_a_measurement_of_the_conversation_it_replaced`
`verified-by: bravebot_config::lib::a_default_budget_is_marked_as_guessed`
`verified-by: bravebot_config::lib::an_advertised_budget_is_not_marked_as_guessed`
`verified-by: bravebot_config::lib::a_budget_set_by_hand_is_not_marked_as_guessed`
`verified-by: bravebot_config::lib::a_window_nobody_advertised_leaves_an_adopted_budget_standing_and_marks_it_guessed`

<a id="INPUT-23"></a>
### INPUT-23: the box edits the ordinary way or vi's, and only a person chooses which

The style is a preference about the person, so it outlives the session that chose it and applies in
every directory. A choice they made outranks a settings file, the file answers for somebody who has
never made one, and with neither the box is the ordinary one. The choice and the setting are both a
word, and a word naming no style is no choice at all whichever of the two spelled it: a settings file
that names none leaves the ordinary box, a record that names none leaves the file answering, and
neither stops anything from starting.

**A choice is made from a panel `/config` opens**, over the transcript, listing the styles with what
each one means and marking the one in force. Enter takes the row under the cursor and says so on the
transcript; Escape leaves the style alone. Whichever style is chosen, the box comes back taking
letters as letters.

Vi editing has two modes over the same line. INSERT is the box everybody has, where a typed
character lands at the caret. NORMAL takes a letter as an instruction, and a letter it has no
instruction for does nothing at all rather than being typed. Every session opens in INSERT.

The ordinary box is in neither mode, and nothing about a mode is drawn at it.

**Why.** Somebody who edits text in vi reaches for `hjkl` before reading anything, and the cost of
the habit meeting a box without it is a prompt full of stray letters. It is a habit rather than a
property of a checkout, which is why the choice is the person's and why a file in a repository is
the weaker claim.

Opening in NORMAL is the one arrangement worth ruling out. The first sentence somebody typed would
go nowhere, and a box that swallows what is typed into it cannot be told apart from one that has
stopped working.

A letter with no instruction does nothing because the mode is not typing. Falling back to inserting
it would make NORMAL mode a place where half the alphabet quietly edits the prompt, and the person
would find out by reading the line rather than by pressing the key.

`verified-by: bravebot_tui::vim::the_configured_word_for_vi_editing_is_the_one_other_tools_use`
`verified-by: bravebot_tui::vim::the_configured_word_is_read_whatever_its_case`
`verified-by: bravebot_tui::vim::a_word_naming_no_style_is_no_choice_at_all`
`verified-by: bravebot_tui::state::a_box_that_edits_vis_way_still_opens_taking_letters_as_letters`
`verified-by: bravebot_tui::state::the_ordinary_box_is_in_no_vi_mode_and_cannot_enter_one`
`verified-by: bravebot_tui::state::a_letter_typed_in_normal_mode_does_not_reach_the_line`
`verified-by: bravebot_tui::state::a_configured_style_is_adopted_and_an_unknown_word_is_not`
`verified-by: bravebot_tui::state::choosing_a_style_of_editing_leaves_the_box_taking_letters`
`verified-by: bravebot_session::store::a_stored_style_of_editing_is_read_back_without_its_newline`
`verified-by: bravebot_session::store::a_file_naming_no_style_of_editing_is_not_a_choice`
`verified-by: bravebot_tui::persist::a_recorded_style_of_editing_is_read_back_and_a_word_naming_none_is_not`
`verified-by: bravebot_config::settings::a_style_of_editing_resolves_like_any_other_single_value`
`verified-by: bravebot_config::settings::a_blank_value_is_not_a_choice`
`verified-by: bravebot_config::settings::a_style_of_editing_is_among_the_names_reported`
`verified-by: bravebot_tui::config_prompt::the_picker_opens_on_the_style_in_force`
`verified-by: bravebot_tui::config_prompt::the_style_in_force_is_marked_wherever_the_cursor_is`
`verified-by: bravebot_tui::config_prompt::the_cursor_stops_at_the_ends_of_the_list`
`verified-by: bravebot_tui::config_prompt::escape_and_ctrl_c_leave_the_style_alone`
`verified-by: bravebot_tui::config_prompt::the_arrows_and_vis_own_keys_walk_the_list`
`verified-by: bravebot_tui::config_prompt::enter_takes_the_row_under_the_cursor`
`verified-by: bravebot_tui::config_prompt::every_row_says_what_it_means`

<a id="INPUT-24"></a>
### INPUT-24: Escape takes the letters as instructions, and takes nothing else

In vi's style Escape enters NORMAL mode and leaves the line exactly as it was. Ctrl-`[` is the same
request from a terminal that reports the modifier rather than sending the byte Escape already is,
and both are answered. Pressed in NORMAL mode the key is claimed and does nothing.

A turn in flight is still stopped first, and discarding a half-typed line is still Ctrl-C. In the
ordinary style Escape discards the line as it always has (INPUT-4).

The mode the box is in is drawn beneath it, beside the mode that says what the session asks before
it acts, and is given up only after everything that is not a mode.

**Why.** These are two presses of one key in the same box, and a key that both entered a mode and
threw a paragraph away would be one nobody could press safely. Somebody reaching for NORMAL mode
would lose a prompt each time, and the way to find out is to have already lost one.

Answering one spelling of the chord works on one machine and does nothing on the next, which reads
as a broken key rather than as a terminal difference.

The mode earns its place beneath the box because it decides whether the next letter is a letter. A
person who cannot see that they are in NORMAL mode is looking at a box that has apparently stopped
taking what they type, and that is the same failure opening in NORMAL would cause.

`verified-by: bravebot_tui::app::escape_enters_normal_mode_without_discarding_the_line`
`verified-by: bravebot_tui::app::either_spelling_of_escape_enters_normal_mode`
`verified-by: bravebot_tui::app::the_chord_that_enters_normal_mode_does_nothing_to_the_ordinary_box`
`verified-by: bravebot_tui::app::escape_still_stops_a_turn_before_it_enters_normal_mode`
`verified-by: bravebot_tui::state::leaving_insert_mode_puts_the_caret_on_a_character`
`verified-by: bravebot_tui::render::the_hint_line_says_which_vi_mode_the_box_is_in`
`verified-by: bravebot_tui::render::the_hint_line_says_nothing_about_a_box_that_edits_the_ordinary_way`
`verified-by: bravebot_tui::render::the_key_list_says_what_escape_does_in_the_box_it_is_drawn_over`

<a id="INPUT-25"></a>
### INPUT-25: six keys open INSERT mode, each saying where the caret lands

`i` before the character the caret is on, `I` at the first character of the line, `a` after the
character the caret is on, `A` at the end of the line, `o` on a new line below, `O` on a new line
above. Opening a line leaves the caret on the new one.

Leaving INSERT mode puts the caret on a character rather than past the end of the line, since in
NORMAL mode the caret sits on the character the next instruction acts on.

`!` and `?` are instructions in NORMAL mode rather than the marks that arm shell mode and put the
key list up (INPUT-2, INPUT-13). Both are a press of `i` away.

**Why.** These are the keys somebody's hands already know, and the only thing that distinguishes
them is where the caret ends up, which is why they are one set rather than six unrelated bindings.

The caret cannot rest past the end of the line because there is no character there for an
instruction to act on, and a block drawn over the column after the line says the next press will
take something that is not there.

Reading `!` as the shell mark would arm a mode from a press asking for something else, and there is
no way out of a shell armed by accident except deleting back past the mark. The same press in
INSERT mode still arms it, which is where somebody who wanted a command is.

`verified-by: bravebot_tui::vim::the_keys_that_open_insert_mode_say_where_the_caret_lands`
`verified-by: bravebot_tui::vim::a_letter_that_means_nothing_in_normal_mode_types_nothing`
`verified-by: bravebot_tui::state::the_keys_that_open_insert_mode_land_the_caret_where_vi_does`
`verified-by: bravebot_tui::state::opening_a_line_leaves_the_caret_on_the_new_one`
`verified-by: bravebot_tui::state::the_shell_marker_is_not_armed_from_normal_mode`
`verified-by: bravebot_tui::state::the_key_list_is_not_opened_from_normal_mode`
`verified-by: bravebot_tui::state::leaving_insert_mode_puts_the_caret_on_a_character`

<a id="INPUT-26"></a>
### INPUT-26: the motions move the caret and nothing else, and never rest inside a marker

| Keys | Where the caret goes |
|---|---|
| `h`, `l`, Space | one character left or right |
| `w`, `e`, `b` | the start of the next word, the end of this word or the next, the start of this word or the previous |
| `0`, `$`, `^` | the first column, the last character, the first character that is not a blank |
| `gg`, `G` | the first line of the input, the last |
| `f`, `F`, `t`, `T` then a character | the next or previous occurrence of it on this line, landing on it or stopping one short |
| `;`, `,` | the last such jump again, and the same jump reversed |

The caret comes to rest on a character and never in the column after the line, since NORMAL mode's
caret sits on the character the next instruction acts on. A jump looks only along the line the caret
is on, and one that finds nothing leaves the caret where it was. Repeating with nothing to repeat
does nothing.

A marker is crossed whole by every one of these, and there is no position inside one for a motion to
leave the caret at.

**Why.** These are the keys somebody's hands already know, so what they do here has to be what they
do everywhere else. `w` lands on the first character of the next word rather than after the word it
crossed, which is where the word keys under Ctrl land: both are wanted, and the letter has to mean
vi's.

A jump crossing a newline would land off the row being read, which is not what a key for reaching a
bracket in front of you is for. Leaving the caret at the end of the line when the character is not
there would move it on a press that failed.

The marker rule holds because the motions walk through the same caret steps the arrows use, rather
than searching the line's bytes. A motion doing its own arithmetic would have to know the marker
rules itself, and the one that forgot would be the one that put the caret inside a picture: `f]`
names a character a marker is spelled with, and it did exactly that before it walked.

`verified-by: bravebot_tui::vim::space_moves_right_like_the_letter_does`
`verified-by: bravebot_tui::vim::the_four_jumps_to_a_character_differ_only_in_direction_and_where_they_stop`
`verified-by: bravebot_tui::vim::the_press_after_a_jump_key_is_the_character_to_jump_to`
`verified-by: bravebot_tui::vim::reversing_a_jump_changes_its_direction_and_nothing_else`
`verified-by: bravebot_tui::vim::a_pair_beginning_with_g_is_the_start_of_the_input_or_nothing`
`verified-by: bravebot_tui::state::the_character_motions_move_one_character`
`verified-by: bravebot_tui::state::the_word_motions_land_where_vi_lands`
`verified-by: bravebot_tui::state::the_line_motions_reach_the_ends_and_the_first_word`
`verified-by: bravebot_tui::state::the_input_motions_reach_the_first_and_last_line`
`verified-by: bravebot_tui::state::the_jumps_to_a_character_land_on_it_or_just_short_of_it`
`verified-by: bravebot_tui::state::a_jump_to_a_character_stays_on_its_own_line`
`verified-by: bravebot_tui::state::the_repeat_keys_do_the_last_jump_again_and_then_the_other_way`
`verified-by: bravebot_tui::state::a_repeat_with_nothing_to_repeat_does_nothing`
`verified-by: bravebot_tui::state::a_motion_crosses_a_marker_whole`
`verified-by: bravebot_tui::state::no_motion_comes_to_rest_past_the_end_of_its_line`
`verified-by: bravebot_tui::state::a_pair_that_means_nothing_ends_the_wait_rather_than_holding_it`

<a id="INPUT-27"></a>
### INPUT-27: three of vi's letters are the keys they spell, wherever those keys reach

`k` and `j` are Up and Down: they walk the rows of a paragraph, then the prompt history, then the
transcript, exactly as the arrows do. `/` opens the search over the prompts already sent, which is
what Ctrl-R opens.

While a key is waiting for the character to jump to, every press is that character, so `f/` jumps to
a slash and `fj` to a `j`.

**Why.** What these reach is not the line. Answering them by moving the caret would leave the prompt
somebody most wants unreachable from the mode they are in, and a person who pressed `k` on an empty
line would get nothing where the arrow beside it walks their history.

They are answered by translating the letter into the key it stands for, so there is one ladder rather
than two: a second copy would be a second set of conditions about when the history is reachable, and
the two would drift.

`/` opens that search because it is the only search here. A key that searched the line being typed
would be answering a question about a paragraph in a box ten rows tall, while the prompts a person
cannot see scroll away above it.

`verified-by: bravebot_tui::state::the_letters_that_spell_other_keys_are_named_rather_than_acted_on`
`verified-by: bravebot_tui::state::a_key_waiting_for_its_character_claims_the_letters_that_spell_other_keys`
`verified-by: bravebot_tui::app::the_row_keys_walk_a_paragraph`
`verified-by: bravebot_tui::app::the_row_keys_reach_the_prompt_history_at_the_ends_of_the_input`
`verified-by: bravebot_tui::app::a_slash_opens_the_search_over_earlier_prompts`
`verified-by: bravebot_tui::app::the_letters_that_spell_keys_are_typed_in_insert_mode`

<a id="INPUT-28"></a>
### INPUT-28: an operator and an extent, and a marker is taken whole or not at all

`d` takes a stretch out, `c` takes it out and opens INSERT mode where it was, `y` keeps it and leaves
the line alone, `>` and `<` move the line a step from or towards the margin. Each waits for the
stretch to act on:

| Keys | The stretch |
|---|---|
| a motion | from the caret to wherever that motion would take it |
| the operator's own letter doubled | the whole line |
| `D`, `C`, `x`, `s` | to the end of the line, and the character under the caret |
| `Y`, `S` | the whole line |

Whether the character the motion landed on is taken depends on the motion: `de` takes the word's last
letter, `dw` stops before the next word's first. `cw` on a character that is not a blank leaves the
space after it, and on a blank takes it.

`p` and `P` put the register back after and before the caret. A stretch that was whole lines comes
back as a line of its own. `J` makes this line and the one below into one with a single space where
the newline was. `u` puts back what the last change took, one step. `.` does the last change again at
the caret.

A marker is taken whole by every operator, or not at all, and taking one takes the attachment off.

**Why.** One operator over one set of extents is why `dw`, `cw` and `yw` are one idea rather than
three bindings, and why `d$` and `dG` work without being listed: the letter says what happens and the
rest says where.

The inclusive and exclusive motions are vi's distinction and not decoration. `cw` behaving as `ce` is
vi's own special case, kept because the alternative is useless: a word replaced and run into the next
one is never what somebody meant, and typing the space back each time is what the key would cost.
Both were measured against vim rather than reasoned about, since they are facts about what people's
hands expect.

The register is vi's unnamed one and the only one. Named registers are a filing system, and a box
holding one line of thought has nothing to file. It is not the system clipboard, which Ctrl-V owns
and which a person shares with every other window they have open.

Undo is one step, on the same footing as putting a line away: the press that undoes and the keystroke
that will be regretted are one apart, and a depth is a thing to remember. `.` repeats the instruction
rather than what it produced, which is the whole point of the key.

A marker is one thing on the screen and one thing to the person looking at it, so half of one stands
for nothing and text that still reads as an attachment over something no longer attached is the
outcome to rule out. It holds because a stretch is measured between positions the caret could rest
at, and no such position is inside a marker.

`verified-by: bravebot_tui::vim::an_operator_takes_any_motion_as_its_stretch`
`verified-by: bravebot_tui::vim::the_doubled_letter_is_the_whole_line_and_only_its_own`
`verified-by: bravebot_tui::vim::an_operator_over_a_jump_waits_again_for_the_character`
`verified-by: bravebot_tui::vim::a_motion_says_whether_an_operator_takes_the_character_it_landed_on`
`verified-by: bravebot_tui::vim::the_yank_is_the_operator_that_only_reads`
`verified-by: bravebot_tui::state::the_delete_operator_takes_the_stretch_a_motion_names`
`verified-by: bravebot_tui::state::the_character_and_the_line_are_extents_of_their_own`
`verified-by: bravebot_tui::state::the_change_operator_takes_the_stretch_and_starts_typing`
`verified-by: bravebot_tui::state::changing_a_word_leaves_the_space_after_it`
`verified-by: bravebot_tui::state::the_yank_operator_leaves_the_line_alone`
`verified-by: bravebot_tui::state::a_yanked_line_comes_back_as_a_line`
`verified-by: bravebot_tui::state::the_register_goes_back_on_either_side_of_the_caret`
`verified-by: bravebot_tui::state::putting_back_an_empty_register_does_nothing`
`verified-by: bravebot_tui::state::the_line_shifts_by_spaces_and_stops_at_the_margin`
`verified-by: bravebot_tui::state::joining_puts_one_space_where_the_newline_was`
`verified-by: bravebot_tui::state::undo_puts_back_what_a_change_took`
`verified-by: bravebot_tui::state::there_is_nothing_to_undo_after_a_yank_or_before_a_change`
`verified-by: bravebot_tui::state::the_repeat_key_does_the_last_change_again_at_the_caret`
`verified-by: bravebot_tui::state::an_operator_takes_a_marker_whole`
`verified-by: bravebot_tui::state::an_operator_that_takes_a_marker_takes_the_attachment_with_it`

<a id="INPUT-29"></a>
### INPUT-29: a text object is a stretch named by what it is

After an operator, `i` and `a` say the stretch is a thing rather than a distance, and the next press
says which thing: `w` a word, `W` a run of anything that is not a blank, and a quote or either half of
a bracket pair for what lies between them. `i` takes what is inside and `a` takes what surrounds it
too. All on the line the caret is on.

A word object is the run the caret is in, and a run of blanks is a run, so the caret is always in
something. `aw` takes the blanks after the word, or the ones before it where there are none after. A
pair is the one enclosing the caret, or else the next one along the line. `a` over a quote pair takes
the blanks in front of it and over a bracket pair does not.

A key naming no kind of thing does nothing. A marker is taken whole or left alone.

**Why.** `ci(` is what somebody means when they want the arguments replaced, and the alternative is
counting characters to a closing bracket they can see perfectly well.

The pair being the next one along, and not only the enclosing one, is what makes `ci(` work with the
caret on the name in front of the bracket, which is where it usually is.

Three classes of character rather than two, because `w` treats punctuation as a word of its own: in
`src/main.rs` the slashes are part of neither name. `W` is the same machinery with punctuation folded
in, which is the whole of the difference between the two and the reason a path is one object.

The blank rules and the difference between a quote pair and a bracket pair were measured against vim
rather than reasoned about. They are facts about what people's hands expect, and the bracket case is
vim's own inconsistency: what a quote delimits reads as a word, so the blank beside it belongs to it,
where a bracket follows the name it belongs to.

The marker rule needs stating separately here because an object is found by reading the line, not by
walking the caret's own positions like every other stretch. A marker is spelled with brackets and a
digit, so `di[` named the brackets one is written with and left half of it standing for nothing.

`verified-by: bravebot_tui::vim::i_and_a_after_an_operator_name_a_text_object`
`verified-by: bravebot_tui::vim::either_half_of_a_pair_names_the_same_object`
`verified-by: bravebot_tui::vim::a_quote_closes_itself`
`verified-by: bravebot_tui::vim::a_key_naming_no_kind_of_object_means_nothing`
`verified-by: bravebot_tui::state::a_word_is_a_text_object_with_and_without_the_blanks_around_it`
`verified-by: bravebot_tui::state::a_bigword_is_everything_that_is_not_a_blank`
`verified-by: bravebot_tui::state::a_pair_of_delimiters_is_a_text_object`
`verified-by: bravebot_tui::state::a_pair_is_the_one_around_the_caret_or_the_next_one_along`
`verified-by: bravebot_tui::state::a_pair_named_from_its_own_delimiter_is_that_pair`
`verified-by: bravebot_tui::state::a_text_object_works_with_every_operator`
`verified-by: bravebot_tui::state::a_pair_naming_no_kind_of_object_does_nothing`
`verified-by: bravebot_tui::state::a_text_object_over_a_marker_takes_it_whole_or_not_at_all`

<a id="INPUT-30"></a>
### INPUT-30: a stretch can be marked out first, and it is drawn while it is chosen

`v` marks out a stretch character-wise and `V` line-wise. Both ends cover the character they sit on, so
the stretch is never empty. Motions move the end the caret is at, `o` puts the caret at the other end,
and a text object becomes the selection.

An operator there needs no extent and acts on the selection: `x` is `d` and `s` is `c`, having nothing
left to distinguish. `r` replaces every selected character with one, and `~`, `u` and `U` change the
case. A line-wise selection goes into the register as lines.

The key that opened the mode closes it, the other of the two changes which kind is in force, and
Escape abandons the selection. Every operator ends it, and so does an edit of the line it was marked
on, whether the edit came from one of vi's own keys or from a key VISUAL mode does not claim. A press
that deletes nothing has not edited the line and leaves the stretch standing; choosing a style of
editing abandons it along with the mode that showed it.

**The whole marked stretch is drawn**, on every row it crosses, and the caret is not drawn within it.

A selection holding a marker is not replaced character by character: that press does nothing.

**Why.** Marking a stretch out and then saying what to do with it is the other way round from an
operator, and the reason to have both is that the stretch is on the screen while it is being chosen.
Which makes drawing it the whole point rather than a decoration: the next key acts on it, and a person
who cannot see which stretch is guessing. A caret drawn inside a reversed block says nothing, so the
selection takes its place.

`u` meaning lower-case here and undo without a selection is why each mode reads its own table. The
motions fall through to the other table rather than being restated, or a motion added to one would be
missing from the other.

Every operator ending the selection is what stops the next press acting on a stretch again for reasons
nothing on the screen explains. Escape abandoning it is the same rule from the other side.

A marker is not a run of characters to overwrite, and replacing the text either side while leaving it
standing would be a line nobody could read.

An edit ending the selection is what a stretch being a pair of positions costs: nothing about the two
says which line they were taken from, and the keys that edit are mostly not vi's own. Backspace, the
readline bindings, a paste, and the prompt an arrow recalls all reach the box while VISUAL mode is
open, and a selection they left standing would name characters that have moved or gone. Drawing that
is not a stretch drawn wrong but a line read outside itself, and the draw is on every frame. The
stretch is read off the line as it stands for the same reason, rather than trusted to be within it.

A press that deletes nothing is exempt because the stretch it would end is still exactly the one on
the screen, and a key that closed it would be doing something visible while doing nothing to the
line. The style of editing is the other way round: it is chosen away from the box, and the box it
comes back to may have no key that could act on a stretch and no mode to draw one for.

Block-wise selection is a known cost rather than a clause.

`verified-by: bravebot_tui::vim::an_operator_in_visual_mode_acts_on_the_selection`
`verified-by: bravebot_tui::vim::the_letters_the_two_modes_disagree_about`
`verified-by: bravebot_tui::vim::the_motions_mean_the_same_thing_in_both_modes`
`verified-by: bravebot_tui::vim::a_text_object_in_visual_mode_selects`
`verified-by: bravebot_tui::vim::replacing_a_selection_waits_for_the_character`
`verified-by: bravebot_tui::vim::an_object_has_no_character_beyond_it`
`verified-by: bravebot_tui::vim::every_mode_but_insert_takes_letters_as_instructions`
`verified-by: bravebot_tui::state::a_selection_is_marked_out_and_then_acted_on`
`verified-by: bravebot_tui::state::a_selection_covers_the_character_it_opened_on`
`verified-by: bravebot_tui::state::the_line_wise_selection_takes_whole_lines`
`verified-by: bravebot_tui::state::the_case_keys_act_on_the_selection`
`verified-by: bravebot_tui::state::swapping_the_ends_moves_the_other_one`
`verified-by: bravebot_tui::state::a_motion_or_an_object_extends_the_selection`
`verified-by: bravebot_tui::state::the_selection_key_opens_and_closes_and_changes_kind`
`verified-by: bravebot_tui::state::escape_abandons_the_selection`
`verified-by: bravebot_tui::state::an_operator_ends_the_selection`
`verified-by: bravebot_tui::state::an_edit_of_the_line_abandons_the_selection`
`verified-by: bravebot_tui::state::a_visual_key_that_changes_the_line_abandons_the_selection`
`verified-by: bravebot_tui::state::a_press_that_changes_nothing_leaves_the_selection`
`verified-by: bravebot_tui::state::choosing_a_style_of_editing_abandons_the_selection`
`verified-by: bravebot_tui::state::the_selection_is_read_off_the_line_as_it_stands`
`verified-by: bravebot_tui::render::an_edit_under_a_selection_still_draws`
`verified-by: bravebot_tui::state::replacing_a_selection_holding_a_marker_leaves_it_alone`
`verified-by: bravebot_tui::render::the_selection_is_drawn_over_the_whole_stretch`
`verified-by: bravebot_tui::render::a_selection_across_rows_is_drawn_on_all_of_them`
`verified-by: bravebot_tui::render::the_ordinary_box_draws_no_selection`

<a id="INPUT-31"></a>
### INPUT-31: Ctrl-S on a prompt walked back to searches this workspace instead

While the box holds a prompt reached by walking back through the history, Ctrl-S opens the search over
the prompts sent, with the scope already narrowed to this workspace: the scope the search's own Ctrl-S
selects (INPUT-19). It is that search in every other respect, opened on the newest match with nothing
typed into it, closed the same way, and the wide list is one more press of the same key from inside it.
On any other line the key still puts the line away or brings one back (INPUT-17), and a line put away
earlier is still there afterwards. The border of the box names this chord beside Ctrl-R while an older
prompt is being walked back to, and gives the two up one at a time where the row will not hold them
beside which prompt is being shown: this one first, then the search over every prompt.

**Why.** Pressing Up says the wanted prompt is an old one, and of the old ones the prompts sent from
the workspace somebody is sitting in are the likelier answer. Up walks one prompt at a time, which is
no way to reach the hundredth, so a person who has pressed it several times is on a path with no end
and this is the key that takes them off it. The list it opens on has to be worth reading for any of
that to hold, which is what nothing being typed into it is for (INPUT-19).

The key means this here without being remembered, which is what makes it one key rather than two: the
line in the box is one the history put there rather than one the person typed, and the history holds it
already, so putting it away stores a second copy of something stored.

Which prompt is being shown is what only that row says, so it is what the row keeps, and the narrower
scope goes before the search it narrows because that search is the one also written down on the key
list. A title drawn anyway lands on top of the position and cuts it mid-word, which reads as a
rendering fault rather than as a border with no room for all of it.

`verified-by: bravebot_tui::app::ctrl_s_searches_this_workspace_while_an_older_prompt_is_shown`
`verified-by: bravebot_tui::render::how_to_search_the_prompts_is_said_where_somebody_would_look`
`verified-by: bravebot_tui::render::a_border_gives_up_the_ways_in_one_at_a_time`

<a id="INPUT-32"></a>
### INPUT-32: a settings file can move seven chords, and nothing else

A `keybindings` block in `settings.json` names an action and the chord it is to answer, spelled
`ctrl-x`, `alt-o` or `ctrl+x`. It layers per action the way `env` does: a project file moving one
action's key says nothing about the other six. There is no second file and no other spelling of the
block, so one place answers what a key does. Seven actions can be moved, and nothing else can:

- `editor` (default: `ctrl-g`): open external editor for the current prompt.
- `watch` (default: `ctrl-l`): watch background delegate or inspect running actions.
- `scroller` (default: `ctrl-o`): open the transcript scroller.
- `history` (default: `ctrl-r`): open prompt history search.
- `stash` (default: `ctrl-s`): stash the current input line or bring it back.
- `trail` (default: `ctrl-t`): toggle turn execution trail visibility.
- `paste` (default: `ctrl-v`): paste from clipboard.

**A chord has to carry Ctrl or Alt.** Every unmodified key is answered already: a character is
typed into the line, Enter sends, Escape clears it, Tab takes what is offered, and the arrows walk
the caret and the history. Shift over a character is refused as well, because a terminal reports
Shift-A as `A` with Shift held, so a chord written `shift-a` or `ctrl-shift-a` names an event that
never arrives. Four chords are refused while carrying Ctrl: Ctrl-C, which stops and then leaves,
and Ctrl-D, which leaves (INPUT-4), and Ctrl-J and Shift-Enter, which start a line (INPUT-2).

**Why.** The arms that read a configured chord sit above the arm that types, so a letter handed to
an action is a letter that can no longer be written: a settings file could take `x` out of the
alphabet. Refusing the whole unmodified half of the keyboard is one rule a person can hold rather
than a list of the keys that happen to be taken today, and the keys the issue is about, Ctrl-S and
Ctrl-O, are reachable under it.

**Every action is left on a key of its own.** A chord the parser cannot read, or one the box
already answers, leaves that action on its default. So does a chord two actions would both answer,
and both of them give it up rather than one keeping it. Two actions trading chords is not a conflict
and both take what they asked for, since a chord is contested only where some other action still
stands on it once every request has been read.

**Why.** Two actions on one chord is worse than either falling back: the routing reads one of them
first, so the other cannot be reached at all, and the list `?` puts up names the same chord twice
while one of the two lines is a lie. Which action wins would come down to the order the code reads
them in, which is nothing a person could predict from what they wrote, so neither wins.

**A mode reads the chord that opened it.** Inside the search over prompts, the chord that puts a line
away narrows the scope and the one that opened the search closes it (INPUT-19, INPUT-31); inside the
view of what a delegate is doing, the chord that opened the view leaves it. Ctrl-C keeps its own
meaning in both, and the chord an action was moved off of does nothing.

**Why.** Every character narrows the prompt search and bare letters walk the delegate list, so a
chord these modes did not ask the bindings about is not merely unanswered: it is read as the letter
it carries, and the search a person moved a chord to open narrows itself to prompts holding an `s`.

**A configured chord takes precedence over line editing.** When a chord is moved onto one of the
readline editing keys (such as `ctrl-u` or `alt-b`), the action answers rather than the line
editing arm. In vi's normal mode, `/` translates to the chord configured for history search.

**The screen names the chord that answers.** `?` lists the keys from the one place they are written
down (INPUT-13), and the seven rows above are asked of the chord in force rather than spelled out
there. So is every other line that names one: the row saying what brings a stashed line back
(INPUT-17), the border while an older prompt is being walked back to (INPUT-31), the keys under the
search (INPUT-19), the hint saying there is something to watch, the note left where a picture on the
clipboard needs a key of its own, and the scroller's way out
([SCROLL-7](scroller.md#SCROLL-7)). A translated line names the chord by taking it as an argument, so
no catalog has to be revisited when a default moves. Where a clause of this spec or another names one
of the seven, it names the default.

**Why.** A list is worth having only where it is right, and a person reads it at the moment a key
they pressed did nothing. Keeping a second copy for the defaults is the same list twice: the copy
`?` was drawn from had already stopped saying that Ctrl-S searches as well (INPUT-31), and nothing
on the screen would have shown it. The scroller's way out named Ctrl-C in the keys and again in the
meaning beside them, which reads as two different presses. A sentence with the chord written into it
is worse than either, because the words around it are the reason somebody believes it.

`verified-by: bravebot_tui::keybindings::parses_hyphen_and_plus_delimiters`
`verified-by: bravebot_tui::keybindings::reserved_keys_are_rejected`
`verified-by: bravebot_tui::keybindings::a_key_the_box_already_answers_is_not_on_offer`
`verified-by: bravebot_tui::keybindings::a_chord_carrying_ctrl_or_alt_is_on_offer`
`verified-by: bravebot_tui::app::a_settings_file_cannot_take_a_letter_away_from_typing`
`verified-by: bravebot_tui::keybindings::invalid_chord_falls_back_to_default`
`verified-by: bravebot_tui::keybindings::conflicting_chords_fall_back_to_defaults`
`verified-by: bravebot_tui::keybindings::no_two_actions_are_left_on_one_chord`
`verified-by: bravebot_tui::keybindings::two_actions_can_trade_chords`
`verified-by: bravebot_tui::keybindings::unknown_actions_in_map_are_ignored`
`verified-by: bravebot_tui::keybindings::custom_chords_override_defaults`
`verified-by: bravebot_config::settings::a_keybindings_block_is_read_from_settings`
`verified-by: bravebot_config::settings::a_keybindings_entry_that_is_not_a_chord_is_dropped`
`verified-by: bravebot_config::settings::a_project_layer_overrides_keybindings_per_name`
`verified-by: bravebot_config::settings::a_local_layer_overrides_project_and_global_keybindings`
`verified-by: bravebot_tui::render::the_shortcut_list_reflects_custom_keybindings`
`verified-by: bravebot_tui::render::the_stashed_line_names_the_custom_stash_chord`
`verified-by: bravebot_tui::render::the_help_names_the_chord_the_scroller_was_opened_with`
`verified-by: bravebot_tui::render::the_help_names_every_key_that_closes_the_scroller`
`verified-by: bravebot_tui::render::how_to_search_the_prompts_is_said_where_somebody_would_look`
`verified-by: bravebot_tui::history_search::the_keys_under_the_search_name_the_chord_that_narrows_it`
`verified-by: bravebot_tui::render::the_row_that_says_what_the_turn_is_doing_leaves_the_key_to_the_hint_line`
`verified-by: bravebot_tui::render::a_picture_on_the_clipboard_says_which_key_carries_it`
`verified-by: bravebot_tui::app::a_moved_chord_is_read_inside_the_search_it_opened`
`verified-by: bravebot_tui::app::a_moved_chord_leaves_the_view_it_opened`
`verified-by: bravebot_tui::app::custom_keybindings_route_actions_and_old_chords_are_ignored`
`verified-by: bravebot_tui::app::custom_keybindings_work_while_a_turn_runs`
`verified-by: bravebot_tui::app::vi_mode_search_prompts_uses_configured_history_chord`
`verified-by: bravebot_tui::app::configured_keybinding_overrides_readline_editing`

<a id="INPUT-33"></a>
### INPUT-33: a session takes the terminal for its length, and gives every part of it back

Starting a session in the interface that draws puts the terminal in raw mode and moves it to a
screen of its own, so what was in the terminal beforehand is untouched and is back on the screen
afterwards. A session in lines ([cli.md](cli.md)) takes none of what follows, and this
clause is about the one that draws. With that screen the
session asks for mouse reporting, narrowed to the buttons, the wheel and motion while a button is
held; bracketed paste; focus reporting; and, only where the terminal says it understands the
request, disambiguated keys.

Every one of those is given back when the session ends, including when it ends by failing rather
than by being left, and again around each handover of the terminal to another program: the editor
a prompt is written in and the viewer a transcript is read in both get the terminal as it was
found, and the same set is taken again on the way back.

**Why.** Each mode is asked for because something here cannot work without it. The wheel scrolls
the transcript only while the mouse is reported, and all-motion reporting is narrowed away because
a pointer merely crossing the window is an event and a redraw per pixel of travel, for a gesture
nothing here reads. Bracketed paste is what stops a pasted prompt sending itself, since without it
the newline most clipboards carry arrives as Enter; it is also the only thing that says a paste came
off a clipboard at all, so where a terminal does not send those markers a paste arrives as bare keys
and nothing downstream can tell it from what a program wrote. Focus reporting is what makes the
clipboard
worth a look at the one moment a picture appears on it, rather than polled for ever. Disambiguated
keys are what make Shift-Enter arrive at all, a terminal otherwise sending the same byte however
Enter was pressed.

Giving them back is owed because none of them is this program's to keep. A mode left on outlives
the process: mouse reporting turns a later click into unreadable bytes, bracketed paste prints its
markers into whatever is typed next, and a keyboard enhancement pushed and never popped sits on a
stack the terminal keeps for every program after this one. The failing exit is the case that
matters most, since a terminal left in raw mode on a screen that is not its own is worse for the
person than whatever error put it there.

`verified-by: bravebot_tui::app::a_session_draws_on_a_screen_of_its_own`
`verified-by: bravebot_tui::app::every_mode_a_session_asks_for_is_given_back`
`verified-by: bravebot_tui::app::a_pushed_keyboard_mode_is_popped_and_an_unpushed_one_is_not`
`verified-by: bravebot_tui::app::the_session_reads_a_drag_and_not_every_pointer_movement`

<a id="INPUT-34"></a>
### INPUT-34: what arrived together is not two presses

A terminal delivers one byte stream and says nothing about who wrote it, so a person at a keyboard and
a program holding the other end of the pty arrive identically and no reader can ask which it has.
**Nothing here tries.** Every event is delivered exactly as the terminal reported it, and what is added
is one fact beside each: whether it was the whole of what was waiting, or arrived together with others.

One event waiting is what a person pressing a key looks like, since nothing fills the buffer between
one read and the next. Two or more were available at the same instant, so **no one of them is evidence
separate from the rest**, whatever they are: a key beside a resize is no more a separate press than two
keys are.

**What asks for it is anything that decides, and nothing else.** The rung that ends a session
([INPUT-4](#INPUT-4)), the question a session opens with ([PROMPT-7](prompting.md#PROMPT-7)), and the
return that takes the line out of the box. Each of those grants or spends something a person cannot get
back, and a program able to write bytes at the terminal writes the key that does it: an editor typing a
virtualenv activation ends its line with a return, and taking that return sent a line nobody wrote to
the planner. Every other reader answers the event it was given, exactly as before this existed, because
moving the caret or narrowing a list costs nothing if a program does it.

**The line stays where it is when the return is refused**, and the refusal is said. Words that vanish
leave somebody with no account of what happened, and a key that does nothing without a word reads as an
interface that has stopped answering. So the text can be read and then sent deliberately or cleared,
which is also why a program cannot make it look touched: an arrow and a return in one write are two
keys that arrived with each other, so neither is a press.

**Why it is one reader.** What arrived together can only be seen where the whole of it is visible. A
prompt reading the terminal itself takes the first key of a burst with nothing behind it and believes
it arrived alone, which is the fact this exists to get right, so every reader in the interface goes
through this one rather than calling the terminal.

**What this deliberately does not do.** It does not decide that a person's keystrokes were a program's.
A reader doing that is wrong about a fast typist behind a slow redraw, about tmux and about ssh, since
what lands in one read is decided by everything between the keyboard and this program rather than by
how fast anybody typed; and being wrong that way costs somebody their own line. Nothing is withheld,
reclassified or delayed. **So this does not stop a program that writes at the terminal from answering a
question**, and nothing here should be read as claiming it does: a question answered by a bare letter
is answered by a bare letter whoever wrote it. What it buys is the one distinction a terminal leaves
available, spent in the one place where a second press is the whole of what is being asked for.

`verified-by: bravebot_tui::input::the_flag_starts_out_saying_a_key_arrived_alone`
`verified-by: bravebot_tui::app::two_interrupts_that_arrived_together_do_not_end_the_session`
`verified-by: bravebot_tui::app::two_end_of_transmissions_that_arrived_together_do_not_end_the_session`
`verified-by: bravebot_tui::app::a_press_on_its_own_after_a_run_still_leaves`
`verified-by: bravebot_tui::app::a_return_that_arrived_with_other_keys_does_not_send`
`verified-by: bravebot_tui::app::every_refused_return_is_said_and_not_just_the_first`
`verified-by: bravebot_tui::app::a_refused_way_out_says_so`
`verified-by: bravebot_tui::app::a_return_of_its_own_still_sends`
`verified-by: bravebot_tui::app::a_program_cannot_send_its_own_line_with_an_arrow_and_a_return`

---
id: ASK
title: ask_user
status: normative
governs:
  - crates/core/src/ask.rs
  - crates/tui/src/ask.rs
  - crates/cli/src/plain.rs
  - crates/ui-bridge/src/turn.rs
  - crates/i18n/src/ask.rs
  - crates/agent/src/settled.rs
  - crates/ui-bridge/src/wire.rs
  - ui/src/renderer/asking.ts
  - ui/scripts/asking.test.mjs
  - ui/scripts/drive-manual-walkthrough.mjs
guards:
  - symbol: Policy::record_answers
documented-by: docs/website/docs/reference/tools.md
---

## Scope

`questions` is routing; the only content argument is the `why` every tool takes ([TOOL-5](tool-surface.md#TOOL-5)). The result is what the user answered,
question by question. This is the one tool whose result comes from a person rather than from the
workspace, and the
only one with no effect at all.

## Clauses

<a id="ASK-1"></a>
### ASK-1: the questions are routing, approved by being read

`ask_user` still has a destination, the user's screen, and the questions and their options decide
what the user is shown and therefore what they can answer. What is drawn is exactly the bytes the
gate checked; nothing re-parses them afterwards, and there is no effect to endorse beyond the
display.

`verified-by: bravebot_core::ask::a_shaped_question_carries_the_tag_it_is_shown_under`
`verified-by: bravebot_core::ask::a_question_with_no_options_is_still_a_question`
`verified-by: bravebot_tui::ask::options_whose_labels_wrap_are_fitted_by_the_rows_they_draw`
`verified-by: bravebot_tui::ask::a_wrapping_list_scrolls_to_the_option_under_the_cursor`

<a id="ASK-2"></a>
### ASK-2: one to four questions, refused whole rather than trimmed

One call carries one to four questions. More is refused, never trimmed. The gate runs **once**,
over a string covering every question in the call.

**Why.** Checking them one at a time would mean deciding, per question, whether that one is put to
the user, and which half of a set survives would then be a decision taken from what is in it. A
question quietly dropped would be one the planner was told the user had been asked and they never
saw.

`verified-by: bravebot_core::ask::a_series_key_covers_every_question_in_it`
`verified-by: bravebot_core::ask::a_series_key_says_how_many_questions_are_in_it`
`verified-by: bravebot_core::ask::shaping_a_series_never_drops_a_question`
`verified-by: bravebot_core::policy::a_refused_series_yields_no_answer_at_all`
`verified-by: bravebot_agent::tools::more_than_four_questions_are_refused_rather_than_trimmed`
`verified-by: bravebot_agent::tools::exactly_four_questions_are_asked_whole`
`verified-by: bravebot_agent::tools::the_gate_runs_once_for_a_call_however_many_questions_it_holds`

<a id="ASK-3"></a>
### ASK-3: asking stops once the planner's context has met something untrusted

The questions carry the integrity of the planner's context. Once that context is untrusted, the
routing gate refuses them and the planner is told so and continues without an answer.

**Why.** A question shown to the user may have been written from bytes an attacker controlled, and
choosing among strings an attacker wrote does not make those strings trustworthy. Treating the
keypress as though it did would carry injected text into the planner's context.

**This cannot happen today.** A context only becomes untrusted by resuming one that already was,
and nothing makes one untrusted in the first place: untrusted content is quarantined rather than
shown, and the only place the context absorbs anything is where content was trusted enough to show.
The gate is here anyway, because what makes that closure safe to rely on is that something refuses
if it ever stops holding. If a change ever lets untrusted bytes into the planner's context, this is
what catches it.

Only integrity decides. A planner that was shown private content writes private questions
([LABEL-8](../labels.md#LABEL-8)), and they are still asked: they go to the screen of the person
whose data it is and nowhere else, which is where a private slot is shown when it is put to them
for vetting.

`verified-by: bravebot_core::policy::a_series_from_an_untrusted_context_cannot_be_put_to_the_user`
`verified-by: bravebot_core::policy::answers_to_an_untrusted_series_are_refused`
`verified-by: bravebot_core::policy::untrusted_questions_are_refused_whatever_their_confidentiality`
`verified-by: bravebot_core::policy::private_questions_are_asked_and_the_reply_stays_private`
`verified-by: bravebot_agent::mcp::writing_a_file_and_asking_a_question_still_work_after_a_vet`

<a id="ASK-4"></a>
### ASK-4: a quarantined read does not stop the planner asking

Context integrity falls when the planner is **shown** something untrusted, never when a turn reads
something. A quarantined read hands the planner a reference and never the bytes, so nothing in
that file could have shaped the question.

**Why.** Lowering integrity at the read would label the planner's own words untrusted on the
strength of a file it never saw, and presentation would then quarantine the planner from itself.
Never move this back to the observation.

`verified-by: bravebot_agent::turn::a_quarantined_read_does_not_stop_the_planner_asking`
`verified-by: bravebot_agent::turn::what_the_planner_writes_after_a_quarantined_read_stays_trusted`
`verified-by: bravebot_core::policy::a_quarantined_read_leaves_the_planner_able_to_see_its_own_words`

<a id="ASK-5"></a>
### ASK-5: an answer is trusted as a first label, and only for a trustworthy question

The bytes came from the user's keyboard, the same source as the task itself. That is a first label
from provenance, not an upgrade. It is still refused when the question being answered was not
itself trustworthy (ASK-3). It is as private as the questions it answers, since the reply repeats
them.

`verified-by: bravebot_agent::turn::every_answer_in_a_series_reaches_the_planner`
`verified-by: bravebot_core::policy::private_questions_are_asked_and_the_reply_stays_private`

<a id="ASK-6"></a>
### ASK-6: skipping answers a question, and questions are put one at a time

Questions are put one at a time with the position shown, in the terminal picker, the line-by-line
interface and the desktop window alike. A series of one has no position to show. For each question,
the user may pick an option, pick several where allowed, answer in their own words, or skip.
Skipping moves to the next question rather than abandoning the rest, and the planner is told,
question by question, what was answered and what was passed over.

The position is worded once, in `bravebot-i18n`, and the end of the series is decided once, by
`Asking::ends_at` in `bravebot-core`. The picker, the line-by-line interface and the desktop bridge
call the first. The picker and the bridge call the second; the line-by-line interface puts every
question not already answered in turn and has no step to end. The desktop bridge sends each
prompt's position (`place`) and whether settling it ends the series (`last`), so the window keeps
only which question it is on, its drafts and its focus. What the planner is told is unchanged by
[ASK-13](#ASK-13), which draws each question and its outcome on the screen once the series is settled.

`verified-by: bravebot_tui::ask::the_position_in_the_series_is_shown`
`verified-by: bravebot_tui::ask::a_lone_question_is_not_labelled_one_of_one`
`verified-by: bravebot_i18n::ask::a_question_is_placed_among_those_put_and_a_lone_one_is_not`
`verified-by: bravebot_i18n::ask_in_french::the_position_of_a_question_is_said_in_the_chosen_catalog`
`verified-by: bravebot_core::ask::a_series_ends_at_its_last_question_and_not_before`
`verified-by: bravebot_ui_bridge::wire::each_prompt_says_where_it_stands_and_whether_settling_it_ends_the_series`
`verified-by: bravebot_tui::ask::answering_one_question_moves_on_to_the_next`
`verified-by: bravebot_tui::ask::skipping_a_question_moves_on_rather_than_abandoning_the_series`
`verified-by: bravebot_tui::ask::skipping_the_last_question_still_ends_the_series`
`verified-by: bravebot_cli::plain::each_question_in_a_series_is_put_under_its_position`
`verified-by: bravebot_cli::plain::the_position_counts_the_questions_put_and_not_those_remembered`
`verified-by: bravebot_cli::plain::a_lone_question_is_put_without_a_position`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so it is pinned instead by ui/scripts/asking.test.mjs, which asserts that a series ends where the bridge says its last question is whether that was answered or skipped, that the answers keep the order the questions were put in with a skip as an empty answer, that the card draws the first question of a series under the position the bridge sent and none of the others, that its button follows the bridge's last flag and not a count the card keeps, and that a lone question carries no position, that progress kept for a series comes back for that series and no other, and by the walkthrough in ui/scripts/drive-manual-walkthrough.mjs, which drives a real window and bridge through a series of three: each question alone on screen under its position, nothing picked or typed leaving the answer shut, a skip moving on to the next question, the card drawn again at the same question with its pick after another conversation was opened, and the planner reading one paragraph per question with the skipped one as a decline; make check-ui runs both, and the governs list above holds the files to existing)`
`verified-by: bravebot_core::ask::a_skipped_question_is_reported_as_declined_beside_its_answered_siblings`
`verified-by: bravebot_core::ask::a_series_is_reported_question_by_question`
`verified-by: bravebot_core::policy::declining_is_an_answer_rather_than_a_refusal`
`verified-by: bravebot_core::policy::fewer_answers_than_questions_are_read_as_declines`
`verified-by: bravebot_core::policy::more_answers_than_questions_are_dropped`
`verified-by: bravebot_core::ask::an_answer_with_no_question_is_dropped_rather_than_attributed_to_one`
`verified-by: bravebot_agent::turn::a_skipped_question_comes_back_as_a_decline_beside_the_rest`

<a id="ASK-7"></a>
### ASK-7: where nobody can be asked, every question is declined

In a one-shot `bravebot "..."` run, every question in the series is declined rather than answered
on the user's behalf. A run given `--input-format lines` puts the questions to the program sending
its lines instead, which answers by position among the choices offered, and in words of its own only
where the person typed `--trust-input-lines` and the question has one prompt, in which case the
trail records the words as arriving on a vouched channel and not as typed
([CLI-36](../cli.md#CLI-36)).

**Why.** The planner is told the reply came from a person, so inventing one would be worse than
not asking.

`verified-by: bravebot_agent::turn::an_unattended_run_declines_every_question_in_the_series`
`verified-by: bravebot_cli::main::a_one_shot_declines_every_question_even_where_it_may_ask_about_a_plan`

<a id="ASK-8"></a>
### ASK-8: an answer is remembered for the session, question by question

A planner that loops back over the same decision does not make the user restate it, and a set
where some are already settled shows only the rest. Questions differing anywhere, including in
their tag, are different questions. `/clear` and `/branch` begin another session, and the answers
given before do not carry into it. An undo (`/undo` and `/rewind` in the terminal, `session.rewind`
in the desktop application) forgets every answer, those given before the turns it undid as well as
in them, because an answer is not kept with the turn it was given in; an undo that finds no turn to
go back to forgets none. The desktop application keeps them the same way: for the
session, across its turns, and a session opened, resumed or forked there begins with none. The
turn's notices name each question it did not put. The terminal and the desktop application
remember only an answer the person gave, so a question that nobody could be asked, or that the
interface returned no answer for, is put again, as is one closed on a clock
([ASK-9](#ASK-9)), which nobody answered. A session in lines reads the end of its input as a
decline and remembers it, as it does a line left empty. The plan put by `propose_plan` is not one
of these questions: it is a permission to change mode, and it is put under a key of its own every
time, so a plan proposed again is asked again ([MODE-14](../permission-modes.md#MODE-14)). A question
answered from memory is not put and so draws no block of [ASK-13](#ASK-13) beside the note that it
was answered already.

`verified-by: bravebot_core::ask::the_key_distinguishes_questions_that_differ_anywhere`
`verified-by: bravebot_core::ask::the_key_distinguishes_questions_that_differ_in_their_tag`
`verified-by: bravebot_core::ask::no_question_can_spell_the_key_of_another`
`verified-by: bravebot_core::ask::a_choice_with_no_detail_differs_from_one_with_an_empty_detail`
`verified-by: bravebot_core::ask::the_key_is_stable_for_the_same_question`
`verified-by: bravebot_agent::tools::the_same_plan_proposed_again_is_put_under_a_new_key`
`verified-by: bravebot_core::ask::two_series_holding_the_same_questions_in_a_different_order_have_different_keys`
`verified-by: bravebot_core::ask::a_question_with_its_choices_reordered_is_a_different_question`
`verified-by: bravebot_core::ask::a_series_answered_entirely_from_memory_asks_nothing`
`verified-by: bravebot_core::ask::remembered_answers_keep_their_places_beside_the_fresh_ones`
`verified-by: bravebot_core::ask::a_series_is_settled_question_by_question`
`verified-by: bravebot_core::ask::a_series_with_some_questions_settled_puts_only_the_rest`
`verified-by: bravebot_core::ask::a_skipped_question_is_remembered_as_skipped`
`verified-by: bravebot_core::ask::a_question_the_interface_left_unanswered_is_put_again`
`verified-by: bravebot_tui::ask::a_question_the_person_answered_is_not_drawn_again`
`verified-by: bravebot_tui::ask::a_series_with_some_questions_settled_draws_only_the_rest`
`verified-by: bravebot_tui::ask::a_question_the_picker_left_unanswered_is_drawn_again`
`verified-by: bravebot_core::ask::a_question_closed_on_the_clock_is_not_remembered_and_the_one_beside_it_is`
`verified-by: bravebot_tui::ask::a_question_closed_on_the_clock_is_asked_again_and_the_one_beside_it_is_remembered`
`verified-by: bravebot_tui::state::clearing_the_conversation_forgets_the_answers_to_the_planners_questions`
`verified-by: bravebot_tui::app::undoing_a_turn_forgets_the_answers_to_the_planners_questions`
`verified-by: bravebot_cli::plain::a_question_asked_again_in_a_session_in_lines_is_answered_from_memory`
`verified-by: bravebot_cli::plain::a_series_with_some_questions_settled_puts_only_the_rest`
`verified-by: bravebot_ui_bridge::answering_tests::a_question_the_person_answered_is_not_put_to_the_window_again`
`verified-by: bravebot_ui_bridge::answering_tests::a_series_with_some_questions_settled_puts_only_the_rest_to_the_window`
`verified-by: bravebot_ui_bridge::answering_tests::a_question_the_window_left_unanswered_is_put_again`
`verified-by: bravebot_ui_bridge::answering_tests::a_question_reused_twice_in_a_turn_is_named_once`
`verified-by: bravebot_ui_bridge::answering_tests::a_question_the_person_skipped_is_not_put_again`
`verified-by: bravebot_ui_bridge::answering_tests::a_question_nobody_could_be_asked_is_not_remembered_as_declined`
`verified-by: bravebot_ui_bridge::answering_tests::a_series_nobody_could_be_asked_keeps_the_answers_given_before`
`verified-by: bravebot_ui_bridge::remembered_answers::a_question_answered_in_one_turn_is_not_put_again_in_the_next`
`verified-by: bravebot_ui_bridge::remembered_answers::a_question_that_differs_only_in_its_tag_is_put_again`
`verified-by: bravebot_ui_bridge::remembered_answers::another_session_is_not_answered_from_the_first_ones_memory`
`verified-by: bravebot_ui_bridge::remembered_answers::a_question_answered_in_an_undone_turn_is_put_again`
`verified-by: bravebot_ui_bridge::remembered_answers::an_undo_that_goes_back_nowhere_keeps_the_answers`
`verified-by: bravebot_ui_bridge::remembered_answers::a_turn_that_fails_after_a_remembered_answer_still_names_the_question_it_did_not_put`

<a id="ASK-9"></a>
### ASK-9: a question nobody answers in the time the person set closes with no answer

The `askIdleSeconds` setting is a whole positive count of seconds. Left unset, or set to zero or to
anything that is not a whole count, a question waits for as long as it takes. Where it is set and no
key has been pressed for that long, the terminal closes the question on screen and every question
behind it with `TimedOut`, which is neither an answer nor a decline. The planner is told, in words
the driver writes, that no answer was given in the time allowed and that the person may be away, so
it can go on with its own judgment and ask again later. Answers already given in the series are
kept. A mark or half-typed text the person had not confirmed is not one, since reporting it would
be a reply nobody gave ([ASK-7](#ASK-7)). Any key press starts the time again, for each question of
a series in turn, and a release, the pointer or a change of size does not. The transcript says that
a question was closed, and the question is not remembered ([ASK-8](#ASK-8)). The note the terminal
draws for the series is the one that says so for the series, and the block under each question that
was closed ([ASK-13](#ASK-13)) says `closed on the clock` for that question alone.

Only the questions the planner puts with `ask_user` close this way. A plan to approve, a spend limit
and a repeated call are the driver's own questions, asked of a person who has to be there, and they
wait for as long as it takes whatever the setting says. The setting is read from the person's own
settings file and a `--settings` file outside the project, and a file that arrived with a checkout
is ignored and named by `doctor`.

**Why.** A question with nobody at the keyboard blocks the turn for good, and a person who starts a
long task and leaves finds it stopped at a question. The decision is a clock and a setting the person
wrote, so nothing a question or an answer says reaches it. A checkout does not get to set the clock:
a question closed on it is one the person was not asked, which is why `vetting.auto` is read from the
person's file alone as well.

`verified-by: bravebot_core::ask::a_question_that_timed_out_is_reported_as_no_answer_and_not_as_a_decline`
`verified-by: bravebot_agent::turn::a_question_closed_on_the_clock_comes_back_as_no_answer_beside_the_rest`
`verified-by: bravebot_tui::ask::a_series_left_alone_closes_from_the_question_on_screen_and_keeps_what_was_answered`
`verified-by: bravebot_tui::ask::a_key_that_answers_nothing_still_restarts_the_time`
`verified-by: bravebot_tui::ask::an_event_that_is_not_a_key_press_does_not_restart_the_time`
`verified-by: bravebot_tui::ask::with_no_time_set_a_question_waits_for_as_long_as_it_takes`
`verified-by: bravebot_tui::ask::marks_and_half_typed_words_are_not_an_answer_when_the_time_runs_out`
`verified-by: bravebot_tui::ask::a_key_arriving_as_the_time_is_up_is_still_an_answer`
`verified-by: bravebot_tui::ask::a_key_waiting_when_the_loop_runs_late_is_still_an_answer`
`verified-by: bravebot_core::ask::a_question_closed_on_the_clock_is_not_remembered_and_the_one_beside_it_is`
`verified-by: bravebot_core::ask::what_is_left_to_put_keeps_the_flag_that_lets_it_close_on_the_clock`
`verified-by: bravebot_tui::ask::a_question_closed_on_the_clock_is_asked_again_and_the_one_beside_it_is_remembered`
`verified-by: bravebot_tui::app::adopting_settings_puts_the_question_idle_time_in_force`
`verified-by: bravebot_tui::state::only_the_planners_own_questions_are_closed_on_the_clock`
`verified-by: bravebot_config::settings::a_settings_file_names_how_long_a_question_waits`
`verified-by: bravebot_config::settings::a_checkout_layer_cannot_set_how_long_a_question_waits`
`verified-by: bravebot_agent::tools::the_planners_questions_may_be_closed_when_nobody_answers`
`verified-by: bravebot_agent::tools::a_plan_put_for_approval_is_never_closed_when_nobody_answers`
`verified-by: bravebot_agent::spend_limit::the_spend_limit_question_is_never_closed_when_nobody_answers`
`verified-by: bravebot_agent::repeated_call::the_repeated_call_question_is_never_closed_when_nobody_answers`

<a id="ASK-10"></a>
### ASK-10: the line under the call counts the questions asked

The line a front end draws under the call says how many questions were asked, as `3 questions asked`
or `1 question asked`, whatever the person did with them. It does not count answers. The questions and what became of each
are drawn under it by [ASK-13](#ASK-13), and the line is the same with or without them.

**Why.** The line is read by a person returning to the session. A count of answers tells them their
questions were settled when they were skipped. The kernel puts the replies into words for the planner,
and the line does not read them a second time to count them.

`verified-by: bravebot_agent::tools::the_line_counts_a_skipped_question_as_asked_and_not_as_answered`
`verified-by: bravebot_agent::tools::the_line_counts_the_questions_asked_where_no_reply_came_back`
`verified-by: bravebot_agent::tools::the_line_counts_every_question_asked_where_each_was_answered`
`verified-by: bravebot_agent::tools::the_line_counts_one_question_in_the_singular`
`verified-by: bravebot_agent::turn::the_line_under_a_skipped_question_does_not_call_it_an_answer`

<a id="ASK-13"></a>
### ASK-13: the screen shows each question put and what became of it

Once the questions of a call are settled, the front end draws, under the call, one block for each
question that was put: its tag and sentence, then the options picked, the words typed, `skipped`, or
`closed on the clock` ([ASK-9](#ASK-9)). The line under the call still counts the questions asked
([ASK-10](#ASK-10)) and the blocks follow it. A question a remembered answer settled was not put and
has no block ([ASK-8](#ASK-8)). Where nobody could be asked ([ASK-7](#ASK-7)) and the interface
returned no answers, there are no blocks, since `skipped` under each question would read as the
person's choice.

The blocks are built once, in `bravebot_core::ask::settled_rows`, from the questions that were put
and the answers the interface returned. They are worded in `bravebot-i18n` and assembled in
`bravebot_agent::settled`, which the terminal, the line-by-line interface and the desktop bridge call.
The bridge sends them as the `ask.settled` event. A newline or other control character in a question
or an answer becomes a space, so neither starts a line of its own. The blocks are for the screen: they
are not given to the planner, not written to the session record and not added to the report of
answers ([ASK-6](#ASK-6)).

**Why.** The planner's reply is the only other place the answers appear. A person scrolling back
saw `3 questions asked` and had to ask the planner what they had decided. The rows are drawn from
values the picker or window returned, in the order the questions were put, and nothing branches on the
text of a question or an answer.

`verified-by: bravebot_core::ask::each_question_is_paired_with_what_became_of_it`
`verified-by: bravebot_core::ask::a_row_is_made_for_each_question_shown_whatever_came_back`
`verified-by: bravebot_i18n::ask::each_way_a_question_can_end_has_its_own_words`
`verified-by: bravebot_i18n::ask_in_french::the_ways_a_question_ends_are_said_in_the_chosen_catalog`
`verified-by: bravebot_agent::settled::each_question_is_followed_by_what_became_of_it`
`verified-by: bravebot_agent::settled::a_question_with_no_answer_reads_as_skipped`
`verified-by: bravebot_agent::settled::nothing_typed_or_asked_starts_a_line_of_its_own`
`verified-by: bravebot_agent::settled::the_glimpse_is_the_lines_and_nothing_cut`
`verified-by: bravebot_tui::render::a_settled_ask_user_call_shows_each_question_and_its_answer_without_a_reply`
`verified-by: bravebot_tui::render::a_series_nobody_could_be_asked_draws_no_rows`
`verified-by: bravebot_cli::plain::each_question_put_is_followed_by_what_became_of_it`
`verified-by: bravebot_cli::plain::a_series_wholly_remembered_draws_no_rows`
`verified-by: bravebot_ui_bridge::answering_tests::the_window_is_sent_each_question_put_and_what_became_of_it`
`verified-by: bravebot_ui_bridge::answering_tests::a_window_that_returned_no_answers_is_sent_no_rows`

<a id="ASK-11"></a>
### ASK-11: a series of questions is reviewed before anything is sent, and can be gone back through

In the terminal picker, a series of more than one question ends in a review rather than at its last
answer. The review lists each question, under its tag or its sentence where it has none, beside what
the person picked, typed or skipped, and says that nothing has been sent. Enter sends. A digit reopens
that question, and escape reopens the last, so neither declines anything already answered. Left, or
`h`, goes back one question from the list and then walks the series on again. A question reopened
comes back with its earlier marks or words, and one reopened from the review returns to the review when it is settled, so a person
changing one answer does not walk through the rest. What was marked on the question they left is not
carried. A series of one has no review, and a series answered entirely from memory asks nothing, so
shows none ([ASK-8](#ASK-8)).

Nothing reaches the planner or the session's memory before Enter on the review: the answers leave the
picker only as the one ordered vector it returns, so the memory is written from what was confirmed
and ASK-5, ASK-6 and the report to the planner are unchanged. The blocks of [ASK-13](#ASK-13) are
drawn from the answers the picker returns after the review is confirmed, so a question reopened and
changed shows the answer that was sent. The one other way out is the clock
([ASK-9](#ASK-9)), which keeps every answer given, including the one a reopened question had, and
closes only a question never answered.

**Why.** Without a review, a mistaken keypress on an earlier question cannot be corrected once the
next one appears, and the planner acts on an answer the person did not mean. The review draws only what the person was already shown
and what they wrote, and no key on it depends on any of that text.

`verified-by: bravebot_tui::ask::an_answer_changed_from_the_review_is_the_one_sent`
`verified-by: bravebot_tui::ask::nothing_is_sent_before_the_review_is_confirmed`
`verified-by: bravebot_tui::ask::going_back_reopens_a_question_with_its_earlier_answer`
`verified-by: bravebot_tui::ask::going_back_from_the_first_question_does_nothing`
`verified-by: bravebot_tui::ask::a_question_reopened_from_the_review_returns_to_the_review`
`verified-by: bravebot_tui::ask::escape_on_the_review_declines_nothing`
`verified-by: bravebot_tui::ask::a_digit_past_the_last_question_does_nothing_on_the_review`
`verified-by: bravebot_tui::ask::a_lone_question_has_no_review`
`verified-by: bravebot_tui::ask::the_review_lists_every_answer_beside_its_question`
`verified-by: bravebot_tui::ask::a_long_answer_does_not_push_the_keys_off_the_review`
`verified-by: bravebot_tui::ask::the_way_back_is_offered_only_where_there_is_one`
`verified-by: bravebot_tui::ask::a_review_left_alone_keeps_the_answers_given`
`verified-by: bravebot_tui::ask::a_question_gone_back_to_keeps_its_answer_and_so_do_those_after_it`
`verified-by: bravebot_tui::ask::going_back_by_hand_from_a_reopened_question_walks_the_series_on`
`verified-by: bravebot_tui::ask::a_wide_answer_and_a_long_tag_leave_the_keys_on_the_review`
`verified-by: bravebot_tui::ask::the_answer_remembered_is_the_one_confirmed`

<a id="ASK-12"></a>
### ASK-12: the description says that a question the request answers is not asked, and that each question costs the user's attention

The `ask_user` description says four things, in these words:

- Do not ask a question whose answer the request states.
- Do not ask whether to do what the user asked, or whether to do it automatically, because they
  asked for it.
- What is left to ask is a choice that the request leaves open and that differs in consequence, such
  as which of two accounts to send from.
- Each question you ask costs the user's attention.

They follow the sentences that say to ask only for what the planner cannot find out itself, to look
first, and that a quarantined result does not stop it asking afterwards ([ASK-4](#ASK-4)). The four sentences are the same on every turn. They name no tool, so a turn
without `run` reads them unchanged ([TOOL-6](tool-surface.md#TOOL-6)), and they name no mode,
classifier or other thing that answers a prompt, so they do not change when the person changes mode
([MODE-8](../permission-modes.md#MODE-8)). The planner is told of no mode but plan mode
([MODE-3](../permission-modes.md#MODE-3), [MODE-8](../permission-modes.md#MODE-8)), and these
sentences add none. The four sentences say nothing about what decides a question
([AUTO-7](../auto-mode.md#AUTO-7)) and nothing about whether a person is shown it. The description also says
the questions "are put to the user one at a time", and that sentence is not one of the four.
The fourth sentence says what a question costs and does not say where it goes or what becomes of it,
because a one-shot run declines every question ([ASK-7](#ASK-7)) and a question closes with no
answer where `askIdleSeconds` is set ([ASK-9](#ASK-9)).

No permission mode answers a question the planner posed ([MODE-7](../permission-modes.md#MODE-7),
[AUTO-4](../auto-mode.md#AUTO-4)), and this clause does not state that again. The description does
not say it either, since the planner is told nothing about modes. Which questions a mode may answer
is not changed by this clause. The text is fixed in the tool table and is built from nothing the
session read or was given, so the driver branches on no content to produce it.

**Why.** A question the request already answers costs the user's attention and decides nothing, and
a planner that has been told only to ask what it cannot find out itself has no sentence that says a
request can be the answer. A planner asked to send an email from a web mail account has no open
choice in how to send it, so asking how, with "send it automatically" among the options, returns what
the request said. The description cannot say that no mode answers the question, because it names no
mode, so it says what a question costs and which questions are left to ask.

`verified-by: bravebot_agent::tools::ask_user_says_not_to_ask_what_the_request_states_or_whether_to_do_what_was_asked`
`verified-by: bravebot_agent::tools::ask_user_leaves_a_question_for_a_choice_the_request_leaves_open`
`verified-by: bravebot_agent::tools::ask_user_says_every_question_costs_the_users_attention_and_names_no_mode`
`verified-by: bravebot_agent::tools::asking_is_described_as_a_last_resort_after_looking`
`verified-by: bravebot_agent::tools::ask_user_says_that_looking_first_does_not_forfeit_the_question`
`verified-by: bravebot_agent::turn::the_request_carries_the_four_ask_user_sentences_where_a_task_is_to_send_an_email`

## Known costs

- The blocks of ASK-13 are not kept in the session record, so a conversation resumed later shows the
  count line without them.
- Only the terminal picker reviews. The line-by-line interface and the desktop app still settle a
  series question by question, and neither can go back.
- The setting applies to the terminal interface's picker alone. A session in lines (`--plain`) reads
  its answers from its input and has no picker whose quiet time the interface measures, so a question
  there waits for as long as it takes.
- A background session holds a question until a client attaches, so nobody has yet been present and
  not answering, and the clock does not run on it.
- The desktop app draws its own question and has no setting for closing one, so a question there
  waits for as long as it takes.
- The clock does not consult whether the terminal has focus. A person reading the question in
  another window for longer than the time set has it closed under them.

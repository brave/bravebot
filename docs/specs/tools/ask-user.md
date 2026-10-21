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
only which question it is on, its drafts and its focus.

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
on the user's behalf.

**Why.** The planner is told the reply came from a person, so inventing one would be worse than
not asking.

`verified-by: bravebot_agent::turn::an_unattended_run_declines_every_question_in_the_series`
`verified-by: bravebot_cli::main::a_one_shot_declines_every_question_even_where_it_may_ask_about_a_plan`

<a id="ASK-8"></a>
### ASK-8: an answer is remembered for the session, question by question

A planner that loops back over the same decision does not make the user restate it, and a set
where some are already settled shows only the rest. Questions differing anywhere, including in
their tag, are different questions. `/clear` and `/branch` begin another session, and the answers
given before do not carry into it. The desktop application keeps them the same way: for the
session, across its turns, and a session opened, resumed or forked there begins with none. The
turn's notices name each question it did not put. The terminal and the desktop application
remember only an answer the person gave, so a question that nobody could be asked, or that the
interface returned no answer for, is put again, as is one closed on a clock
([ASK-9](#ASK-9)), which nobody answered. A session in lines reads the end of its input as a
decline and remembers it, as it does a line left empty. The plan put by `propose_plan` is not one
of these questions: it is a permission to change mode, and it is put under a key of its own every
time, so a plan proposed again is asked again ([MODE-14](../permission-modes.md#MODE-14)).

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
a question was closed, and the question is not remembered ([ASK-8](#ASK-8)).

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
or `1 question asked`, whatever the person did with them. It does not count answers.

**Why.** The line is read by a person returning to the session. A count of answers tells them their
questions were settled when they were skipped. The kernel puts the replies into words for the planner,
and the line does not read them a second time to count them.

`verified-by: bravebot_agent::tools::the_line_counts_a_skipped_question_as_asked_and_not_as_answered`
`verified-by: bravebot_agent::tools::the_line_counts_the_questions_asked_where_no_reply_came_back`
`verified-by: bravebot_agent::tools::the_line_counts_every_question_asked_where_each_was_answered`
`verified-by: bravebot_agent::tools::the_line_counts_one_question_in_the_singular`
`verified-by: bravebot_agent::turn::the_line_under_a_skipped_question_does_not_call_it_an_answer`

## Known costs

- The setting applies to the terminal interface's picker alone. A session in lines (`--plain`) reads
  its answers from its input and has no picker whose quiet time the interface measures, so a question
  there waits for as long as it takes.
- A background session holds a question until a client attaches, so nobody has yet been present and
  not answering, and the clock does not run on it.
- The desktop app draws its own question and has no setting for closing one, so a question there
  waits for as long as it takes.
- The clock does not consult whether the terminal has focus. A person reading the question in
  another window for longer than the time set has it closed under them.

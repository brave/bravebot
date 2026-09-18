---
id: PROMPT
title: Asking a person
status: normative
governs:
  - crates/tui/src/confirm.rs
  - crates/tui/src/trust_prompt.rs
  - crates/tui/src/remote_confirm.rs
documented-by: docs/website/docs/security/permissions.md
---

## Scope

Every moment the system stops and puts something to a human: what the prompt must show, what an
answer grants, and what one answer must never be taken for. `ask_user`, where the **planner** asks
a question, is [tools/ask-user.md](tools/ask-user.md) and is a different thing: these prompts are the system asking
permission.

There are eight: the startup trust question, a directory a settings file asked for, a read of a file
nobody vouched for, a write or edit, a run, reading what a run printed, being shown one quarantined
slot a check has read, and the plan a manifest run is about to walk.

The last is the only one about a whole run rather than about one thing at the moment it is due. It
can be, because that mode fixes every step while the task string is still the only input
([manifest.md](manifest.md#MANIFEST-10)). Everything below applies to it as it does to the rest.

## What every prompt owes the reader

<a id="PROMPT-1"></a>
### PROMPT-1: a prompt shows what is actually at stake, not a summary of it

A write prompt shows the path and the body; an overwrite shows what it replaces; a run prompt
shows the argv, the resolved binary and the directory; an output prompt shows the bytes and the
command that printed them; a vetting prompt shows the bytes, where they came from, and what the
check said about them rather than the word alone; the prompt about a directory a settings file
asked for shows the path it would open. A person cannot endorse a routing field they were not shown.

Every prompt a check was run for shows what it said, and the three are drawn out of one row builder
rather than three, so a prompt cannot carry a verdict and forget to say what it was. Which prompts
those are is [CHECK-10](vetting.md#CHECK-10).

The plan prompt shows the task in the person's own words and then every step, in order, each naming
its tier, what it would do, and every routing field the step fixes rather than the headline one
alone. Every step, never a count and never the first few: one answer covers all of them, and the
step below the fold is as binding as the first.

`verified-by: bravebot_tui::confirm::a_new_file_prompt_shows_the_path_and_body`
`verified-by: bravebot_tui::confirm::an_overwrite_prompt_shows_what_it_replaces`
`verified-by: bravebot_tui::confirm::a_run_prompt_shows_the_argv_the_binary_and_the_directory`
`verified-by: bravebot_tui::confirm::the_output_prompt_shows_the_bytes_and_the_command`
`verified-by: bravebot_tui::confirm::the_vet_prompt_shows_the_bytes_and_where_they_came_from`
`verified-by: bravebot_tui::confirm::the_output_prompt_says_what_a_check_found`
`verified-by: bravebot_tui::confirm::the_vouch_prompt_says_what_a_check_found`
`verified-by: bravebot_tui::trust_prompt::the_named_prompt_shows_the_directory_it_would_open`
`verified-by: bravebot_tui::trust_prompt::the_keys_that_answer_stay_on_screen_at_ordinary_sizes`
`verified-by: bravebot_tui::confirm::the_plan_prompt_shows_the_task_and_every_step`
`verified-by: bravebot_core::manifest::a_described_step_names_every_routing_field_it_fixes`

<a id="PROMPT-2"></a>
### PROMPT-2: a prompt says what approving does, and what it does not

The run prompt says it is not sandboxed, asks for the side effects and the output together, and
names the exact command it would vouch for. The output prompt says
what approving does. The vetting prompt says what approving does and, because nothing else on the
screen would, what it does not: no path is vouched for, so the same thing read again asks again. The trust prompt explains the consequence and names both answers. The prompt
about a directory a settings file asked for says that opening it grants reach and trust, and that a
file asked for it. The plan prompt says that nothing the run reads can add a step, drop one, or send
anything anywhere the plan does not already name; that approving the plan is not approving its
writes, which are each still put to the person as they come up; and that nothing has happened yet, so
declining leaves everything as it is.

**Why.** The second half of a run grant, that what the command prints becomes trusted, is the one
nothing else would tell the user. A directory nobody typed is the same problem in the other
direction: without the question saying where the name came from, the box is unexplained. A plan is
the same problem at its largest, since a reader has no way to tell from a step list how much of the
run one keypress settles.

`verified-by: bravebot_tui::confirm::a_run_prompt_says_it_is_not_sandboxed`
`verified-by: bravebot_tui::confirm::a_run_prompt_asks_for_the_side_effects_and_the_output_together`
`verified-by: bravebot_tui::confirm::a_run_prompt_names_the_exact_command_it_would_vouch_for`
`verified-by: bravebot_tui::confirm::the_output_prompt_says_what_approving_does`
`verified-by: bravebot_tui::confirm::the_vet_prompt_says_what_approving_does_and_does_not_do`
`verified-by: bravebot_tui::trust_prompt::the_prompt_explains_the_consequence`
`verified-by: bravebot_tui::trust_prompt::the_prompt_names_the_directory_and_both_answers`
`verified-by: bravebot_tui::trust_prompt::the_named_prompt_explains_what_opening_does`
`verified-by: bravebot_tui::confirm::the_plan_prompt_says_what_approving_it_does_and_does_not_do`

<a id="PROMPT-3"></a>
### PROMPT-3: what a prompt shows is drawn inside a margin it cannot forge

Content in a prompt is untrusted like any other. An untrusted body is marked as such,
and command output is drawn inside the margin. So is the content a vetting prompt shows, and so is
the sentence the check wrote about it, on whichever of the three prompts it was written for: that
sentence is free text about bytes an attacker may own, and it is the one line on such a screen a
reader might otherwise take for the program's. The verdict word is the driver's and sits outside the
margin; the sentence is not, and sits inside it. A processor's remark
([PROC-12](processors.md#PROC-12)) is drawn in the transcript's own quarantine block against the
prompt's margin, rather than against the transcript's: one screen with two margin columns on it is a
screen where the column stops meaning anything.

A manifest plan's steps are the one body here drawn without a bar, and the reason for the rule is
what says so: they are the driver's own rendering of a plan that came from a context holding the task
string and the driver's own words, and nothing a step read exists yet. A bar down them would mark as
untrusted the one thing on the screen that is not, which is the opposite of why they can be shown.

The margin is on every **drawn row**, not every line of the content. A command's output is
untrimmed and a hunk of a body has no width cap, so a line wider than the prompt box is ordinary
rather than exotic: it is broken to the box's width by the same step that draws the margin, and
each row it breaks into carries a bar of its own. A continuation row that started at column 0 would
be untrusted content outside the margin, placed where the content's own padding chose, near enough
to paint a bar of its own in the margin column. What does not fit vertically is scrolled to rather
than dropped, as it is for any long body.

`verified-by: bravebot_tui::confirm::an_untrusted_body_is_marked_in_the_prompt`
`verified-by: bravebot_tui::confirm::output_is_drawn_inside_the_margin_it_cannot_forge`
`verified-by: bravebot_tui::confirm::vetted_content_is_drawn_inside_the_margin_it_cannot_forge`
`verified-by: bravebot_tui::confirm::what_the_check_said_is_drawn_inside_the_margin_too`
`verified-by: bravebot_tui::confirm::a_wrapped_output_line_is_marked_on_every_row_it_reaches`
`verified-by: bravebot_tui::confirm::a_wrapped_untrusted_hunk_is_marked_on_every_row_it_reaches`
`verified-by: bravebot_tui::confirm::a_wrapped_vouch_preview_is_marked_on_every_row_it_reaches`
`verified-by: bravebot_tui::confirm::a_wrapped_vetted_line_is_marked_on_every_row_it_reaches`
`verified-by: bravebot_tui::confirm::a_remark_cannot_paint_a_margin_in_the_box_it_is_drawn_in`
`verified-by: bravebot_tui::confirm::a_control_character_in_a_remark_is_replaced`

<a id="PROMPT-4"></a>
### PROMPT-4: a review stays legible, or says it could not

A long body keeps the question on screen and offers the rest, which can be scrolled to. A small
edit in a large file shows only the change. An empty output says so, and so does a slot with nothing in it
and a file with nothing to preview. A diff that cannot be computed says so rather than showing nothing. A plan longer
than the box is scrolled to rather than cut short, and the question stays on screen while it is.
A processor's remark is bounded in **drawn rows**, so a claim cannot push the bytes it is a claim
about below the fold, and the block says how many lines it is not showing.

**Why.** Reviewing a whole file body on a terminal is not review, which is why `edit_file` exists
on a passage rather than a whole body. A prompt that scrolled the question away would be collecting a keypress, not a decision.

`verified-by: bravebot_tui::confirm::a_long_body_keeps_the_question_on_screen_and_offers_the_rest`
`verified-by: bravebot_tui::confirm::the_rest_of_a_long_body_can_be_scrolled_to`
`verified-by: bravebot_tui::confirm::a_small_edit_in_a_large_file_shows_only_the_change`
`verified-by: bravebot_tui::confirm::output_that_is_empty_says_so`
`verified-by: bravebot_tui::confirm::vetted_content_that_is_empty_says_so`
`verified-by: bravebot_tui::confirm::a_preview_with_nothing_in_it_says_so`
`verified-by: bravebot_tui::confirm::an_uncomputable_diff_says_so`
`verified-by: bravebot_tui::confirm::a_long_plan_keeps_the_question_on_screen_and_offers_the_rest`
`verified-by: bravebot_tui::confirm::a_long_remark_does_not_push_the_diff_off_the_screen`

## What an answer means

<a id="PROMPT-5"></a>
### PROMPT-5: one answer is never taken for another

An approved write does not approve a run, and does not approve the plan a write is in. A write
approval is not an answer to a question, and an answer to a question is not consent to a write. An
approval to read what a program printed does not promote a slot, which covers more. Each
endorsement is single-use and bound to the exact value it was given for.

Bound to it means the value tells two plans apart wherever they would do different things, so it
spells the tree a plan runs in, the binary each step resolved to and each destination a step's output
is sent to, every one of them by that path's own bytes. A rendering maps every byte it cannot read
onto one replacement character ([tools/run.md](tools/run.md#RUN-8)), and an endorsement keyed on a
rendering is redeemable by a binary, a tree or a destination the person was never shown.

**Why.** These are separate grants that happen to use the same keyboard. The plan is the widest of
them, so it is the one where taking another answer for it would run the most that nobody was shown.

`verified-by: bravebot_tui::remote_confirm::an_approved_write_does_not_approve_a_run`
`verified-by: bravebot_tui::remote_confirm::an_approved_write_does_not_approve_a_plan`
`verified-by: bravebot_tui::remote_confirm::a_write_approval_is_not_taken_as_an_answer_to_a_question`
`verified-by: bravebot_tui::remote_confirm::an_answer_to_a_question_is_not_taken_as_consent_to_a_write`
`verified-by: bravebot_tui::remote_confirm::an_approved_output_read_does_not_approve_a_vetted_read`
`verified-by: bravebot_core::policy::an_endorsement_is_not_redeemable_by_a_binary_that_only_renders_the_same_way`
`verified-by: bravebot_core::command::a_plan_is_keyed_on_the_bytes_of_the_file_a_name_resolved_to`
`verified-by: bravebot_core::command::the_directory_a_plan_runs_in_is_keyed_on_its_bytes`
`verified-by: bravebot_core::command::where_a_plan_writes_is_keyed_on_its_bytes`

<a id="PROMPT-6"></a>
### PROMPT-6: standing permission needs its own key, and is never the default

The run prompt separates running once from running always, Enter does not approve a run, and a run
releasing private data offers no standing permission at all. Declining, and Ctrl-C, vouch
for nothing.

A third answer, whose grant outlives the session, is [tools/run.md](tools/run.md)'s. Everything
above holds of it: it has a key of its own, Enter does not reach it, a run
releasing private data does not offer it, and declining or Ctrl-C records nothing. Its row is where the
two standing lifetimes are told apart, since a key saying only `always` is the one that could be read
as either.

The plan prompt offers no standing form at all, and Enter does not approve a plan either. A plan is
written afresh for each run, so remembering an answer to one would be approving steps nobody has
seen.

The standing key at a vetting prompt is not an answer to the question it asks. What the question asks
about is one slot's bytes, so there is nothing a standing answer to *that* could be about: a
promotion covers those bytes once and writes no rule. The key turns on auto-vetting
([vetting.md](vetting.md#CHECK-11)), which is a decision about which of two questions later slots
raise, and everything above holds of it: it has a key of its own, Enter does not reach it, and
declining or Ctrl-C turns nothing on.

Both prompts that promote one slot's bytes offer it, and offer it identically: the one over content
the planner asked to be shown, and the one over what a program printed. They ask the same question
about the same kind of grant, so a key present at one and missing at the other would tell a person
only which tool the planner happened to call. The vouch offer does not carry it, because what a yes
there writes is a rule about a path rather than a promotion, and the mode it would turn on does not
reach that question.

**It is offered only where the check completed and found nothing.** A prompt carrying a warning, or
saying the check could not be made, is the worst moment to grant it, which is the reasoning behind
the rule that a run releasing private data offers no standing permission at all. The key is unbound
where it is not drawn, since a key granting something the same screen does not offer is worse than
an unbound one, and this key's grant outlives the session that could have corrected it. What that
costs is on [labels.md](labels.md)'s list: an attacker who can force a safe verdict can put the key
on the screen, and a person still has to press it with the bytes in front of them.

`verified-by: bravebot_tui::confirm::the_run_keys_separate_running_once_from_running_always`
`verified-by: bravebot_tui::confirm::the_run_keys_separate_this_session_from_every_session`
`verified-by: bravebot_tui::confirm::the_row_says_which_lifetime_the_always_key_grants`
`verified-by: bravebot_tui::confirm::enter_does_not_approve_a_run`
`verified-by: bravebot_tui::confirm::enter_does_not_record_a_run_past_the_session`
`verified-by: bravebot_tui::confirm::refusing_a_run_records_nothing_past_the_session`
`verified-by: bravebot_tui::confirm::a_prompt_that_offers_no_record_binds_no_key_to_one`
`verified-by: bravebot_tui::confirm::enter_does_not_approve_a_plan`
`verified-by: bravebot_tui::confirm::enter_does_not_approve_a_vetted_read`
`verified-by: bravebot_tui::confirm::a_safe_verdict_does_not_change_which_keys_the_vet_prompt_offers`
`verified-by: bravebot_tui::confirm::only_a_safe_verdict_offers_to_stop_asking`
`verified-by: bravebot_tui::confirm::pressing_always_at_a_not_safe_vet_prompt_grants_nothing`
`verified-by: bravebot_tui::confirm::refusing_a_vetted_read_turns_nothing_on`
`verified-by: bravebot_tui::confirm::only_a_safe_verdict_offers_to_stop_asking_about_output`
`verified-by: bravebot_tui::confirm::the_standing_key_is_bound_at_the_output_prompt_only_where_it_is_drawn`
`verified-by: bravebot_tui::confirm::every_verdict_still_offers_both_answers_about_output`
`verified-by: bravebot_tui::confirm::a_run_that_releases_private_data_offers_no_standing_permission`
`verified-by: bravebot_tui::confirm::pressing_always_at_a_private_input_prompt_grants_nothing`
`verified-by: bravebot_tui::confirm::a_private_input_run_can_still_be_approved_once_or_refused`
`verified-by: bravebot_tui::confirm::saying_no_to_a_run_vouches_for_nothing`
`verified-by: bravebot_tui::confirm::ctrl_c_refuses_the_run_and_vouches_for_nothing`
`verified-by: bravebot_tui::trust_prompt::declining_trusts_nothing`

<a id="PROMPT-7"></a>
### PROMPT-7: declining is not cancelling

Saying no to a write does not stop the turn; Ctrl-C refuses it and does. Leaving at a question a
session opens with ends the session and opens nothing, and it is the row that leaves which does it
rather than any single key ([PROMPT-11](#PROMPT-11)).

**Why.** A refusal the agent can carry on past is how a person steers without starting over.

`verified-by: bravebot_tui::confirm::saying_no_does_not_stop_the_turn`
`verified-by: bravebot_tui::confirm::ctrl_c_refuses_the_write_and_stops_the_turn`
`verified-by: bravebot_tui::confirm::only_the_interrupt_stops_the_turn_at_a_run_prompt`
`verified-by: bravebot_tui::trust_prompt::ctrl_c_moves_nothing_and_decides_nothing`
`verified-by: bravebot_tui::trust_prompt::leaving_starts_no_session`
`verified-by: bravebot_tui::trust_prompt::leaving_at_one_of_the_questions_opens_nothing`

<a id="PROMPT-8"></a>
### PROMPT-8: a resume restores standing permissions, and nothing else

Three of these grants are standing. Two, the trust map
and the list of commands a person said to stop asking about, are written into the session record and
come back with `--resume`, because the person resuming is the person who gave them; a fresh session
in the same directory restores neither and asks again. The third is not restored by a resume at all:
the record of command lines somebody asked to be remembered past the session, which
[tools/run.md](tools/run.md) governs, is read by every session begun in that directory, resumed or
fresh. It reaches a fresh session because the key that made it said how long its answer lasts, and
because what it carries is the asking rather than any trust.

Nothing else survives. A single-use endorsement is created by one approval, is bound to one value,
and is never written down, so a resumed turn cannot replay a write or a run that an earlier turn
was allowed. Answers to the planner's own questions are remembered only in the live session, so a
resumed session puts them again.

Auto-vetting ([vetting.md](vetting.md#CHECK-11)) is not one of the three and is not in the session
record either. It is not a grant about any particular thing: it says which of two questions a
session asks, in the way `editorMode` says which keys move the caret, and the routes that turn it on
are a flag for one run and two files read at startup. A resumed session reads those the way a fresh
one does, so the record would be a fourth answer for the same question to disagree with.

**Why.** A standing permission is a decision about the future that its owner made deliberately. An
endorsement is a decision about one act that has already happened, and reviving one would be
approving something nobody looked at.

`verified-by: bravebot_core::policy::a_turn_inherits_what_the_session_vouched_for`
`verified-by: bravebot_core::policy::an_endorsement_cannot_be_replayed`
`verified-by: bravebot_agent::turn::a_line_remembered_past_the_session_runs_without_asking`
`verified-by: bravebot_tui::sessions::sessions_are_written_read_back_and_kept_per_directory`

<a id="PROMPT-9"></a>
### PROMPT-9: where nobody can be asked, the answer is no

A one-shot run refuses effects rather than applying them unseen, and declines every question
rather than inventing an answer. A closed channel refuses a run and answers no
question, and it refuses to promote a slot however the check that read it answered: a word from a
model is not a person having read something. A plan is refused too, so a manifest run with nobody to ask stops before its first step
rather than walking a program nobody read.

A person may answer these eight in advance, for a session or for a run, by choosing a mode:
[permission-modes.md](permission-modes.md) is what each mode answers and what asking for one costs.
That is somebody's own standing answer rather than a default, and no mode answers a question the
planner posed, since that asks for information rather than consent.

`--vet` ([vetting.md](vetting.md#CHECK-11)) is such an answer in advance, and to one of the eight
only: with it, a check that completes and finds nothing promotes the one slot the planner asked to
be shown, and nothing else about a run nobody is watching changes. Without it a check on that path
is a model call whose word ends in the refusal above. The closed channel keeps refusing either way:
a channel that cannot carry a question is a session whose person went away mid-turn, not somebody
saying anything in advance.

`verified-by: bravebot_tui::remote_confirm::a_closed_channel_refuses_a_run`
`verified-by: bravebot_tui::remote_confirm::a_closed_channel_refuses_a_vetted_read`
`verified-by: bravebot_tui::remote_confirm::a_closed_channel_answers_no_question`
`verified-by: bravebot_tui::remote_confirm::a_dropped_answer_channel_answers_no_question`
`verified-by: bravebot_tui::remote_confirm::a_refusal_travels_back_too`
`verified-by: bravebot_agent::turn::an_unattended_run_declines_every_question_in_the_series`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_reaches_the_planner_unasked`
`verified-by: bravebot_agent::manifest::a_plan_nobody_approved_runs_nothing`

<a id="PROMPT-10"></a>
### PROMPT-10: a prompt's own chrome is wholly the theme's

Every cell a prompt's border encloses carries the theme's background and its text colour, whatever
is drawn over them. A prompt that painted only its border is a hole in the palette, since clearing
the cells under a panel empties them without colouring them.

**Why.** The boundary between what the system is asking and what somebody else's bytes say is what
a person reads when they answer, and a frame half in the theme and half in the terminal's own
colours is a weaker one. Text that sets no colour of its own is otherwise the terminal's default,
which under a light theme in a dark terminal is the path to unreadable.

`verified-by: bravebot_tui::confirm::every_prompt_paints_the_themes_background_inside_its_border`
`verified-by: bravebot_tui::trust_prompt::the_prompt_paints_the_themes_background_inside_its_border`
`verified-by: bravebot_tui::trust_prompt::the_named_prompt_paints_the_themes_background_inside_its_border`

<a id="PROMPT-11"></a>
### PROMPT-11: a question a session opens with is answered on a row, never by one key

The two questions asked before a session exists, the working directory's and one for each directory
a settings file named, put their answers as rows and take the row under the cursor when Enter is
pressed. **No single key press answers either of them.** A bare letter moves nothing and decides
nothing, and the cursor opens on the row that declines, so the key most likely to be pressed
without reading grants nothing. Ctrl-C and Escape move nothing and decide nothing either. The way
out is the row that says so, reached with the arrows like the rest.

**Why no key points at leaving.** A key that moved the cursor onto the row that leaves without
taking it would be half of the quit gesture, and the other half is an Enter. A program that can
write an interrupt can write a return after it, so the two together spell the whole of it in two
bytes, which is what a shell integration writes when it clears the line before typing a command.
Pointing the cursor at leaving on a keystroke gives back most of what asking for a row buys.

**Why.** A terminal delivers one byte stream and says nothing about who wrote it
([INPUT-34](terminal-input.md#INPUT-34)), so one keystroke is weak evidence that a person made it.
Any program holding the other end of the pty spells a letter sooner or later, and an editor
activating a virtualenv in the terminal it opened is not an exotic one. These two questions grant
reach and trust over a whole tree before any of it has been read, which makes them the worst place
for a stray byte to be taken for a person. A row confirmed by Enter needs a gesture no single write
produces: something has to move the cursor onto the row that trusts and then confirm it, and a
write that only spells words reaches neither step.

**What this does not claim.** A program writing two keys in sequence, an arrow and then Enter, is
indistinguishable from a person doing the same, and nothing here stops it. This raises what the
channel costs rather than closing it. What it does buy is that the cheapest outcome of a stray
write is the answer that grants nothing, since the row under the cursor declines until something
moves it.

`verified-by: bravebot_tui::trust_prompt::the_question_opens_on_declining_so_a_stray_enter_grants_nothing`
`verified-by: bravebot_tui::trust_prompt::the_cursor_opens_on_declining_where_a_person_can_see_it`
`verified-by: bravebot_tui::trust_prompt::no_bare_letter_moves_the_cursor_or_answers`
`verified-by: bravebot_tui::trust_prompt::no_other_control_chord_moves_the_cursor`
`verified-by: bravebot_tui::trust_prompt::ctrl_c_moves_nothing_and_decides_nothing`
`verified-by: bravebot_tui::trust_prompt::escape_moves_nothing_and_decides_nothing`
`verified-by: bravebot_tui::trust_prompt::enter_takes_the_row_under_the_cursor`
`verified-by: bravebot_tui::trust_prompt::the_arrows_walk_the_rows_and_stop_at_their_ends`
`verified-by: bravebot_tui::trust_prompt::a_command_line_another_program_typed_in_answers_nothing`
`verified-by: bravebot_tui::trust_prompt::a_command_line_another_program_typed_in_opens_no_named_directory`

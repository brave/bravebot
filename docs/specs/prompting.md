---
id: PROMPT
title: Asking a person
status: normative
governs:
  - crates/tui/src/confirm.rs
  - crates/tui/src/trust_prompt.rs
  - crates/tui/src/remote_confirm.rs
  - ui/src/renderer/shown.ts
  - ui/src/renderer/components/Transcript.tsx
  - ui/src/renderer/components/Diff.tsx
documented-by: docs/website/docs/security/permissions.md
kept_in:
  - Confirmer::confirm_write: one effect; credential writes agreed to for the session; credential writes remembered past the session
  - Confirmer::confirm_run: one effect; programs vouched for; lines remembered past the session; run prompts drawn; reach kept from a run
  - Confirmer::confirm_read_output: one effect; auto-vetting
  - Confirmer::confirm_vetted_read: one effect; auto-vetting
  - Confirmer::confirm_fetch: one effect
  - Confirmer::confirm_server: language servers started
  - Confirmer::confirm_manifest: one effect
  - Confirmer::confirm_vouch: the trust map
  - Confirmer::confirm_exposing_read: files sent despite a credential
  - Confirmer::confirm_tool_list: MCP tool lists approved
  - Confirmer::confirm_mcp_call: one effect; standing answers to an MCP tool
  - Confirmer::confirm_move: MCP server moves
  - Confirmer::confirm_path: paths reached
  - Confirmer::confirm_host: hosts answered
  - Confirmer::ask_user: answers to the planner's questions
  - Confirmer::interjection: nothing
  - Answers::trust: the trust map
  - Answers::programs: programs vouched for
  - Answers::servers: language servers started
  - Answers::asked_about: run prompts drawn
  - Answers::exposed: files sent despite a credential; credential writes agreed to for the session
  - Answers::rules: granted rules
  - trust_prompt::ask: the trust map; a kept answer to the startup question
  - trust_prompt::ask_named: the trust map
  - trust_prompt::ask_granted: granted rules
---

## Scope

Every moment the system stops and puts something to a human: what the prompt must show, what an
answer grants, and what one answer must never be taken for. `ask_user`, where the **planner** asks
a question, is [tools/ask-user.md](tools/ask-user.md) and is a different thing: these prompts are the system asking
permission.

Which prompts there are, and where the answer to each is kept, is the register in
[PROMPT-11](#PROMPT-11).

One more is proposed and not built: sending a message to another background session
([session-messages.md](session-messages.md#MSG-3)). The register and the mode paragraph below read
without it until it is.

The plan a manifest run is about to walk is the only prompt about a whole run rather than about one
thing at the moment it is due. It can be, because that mode fixes every step while the task string is
still the only input ([manifest.md](manifest.md#MANIFEST-10)). Everything below applies to it as it
does to the rest.

## What every prompt owes the reader

<a id="PROMPT-1"></a>
### PROMPT-1: a prompt shows what is actually at stake, not a summary of it

A write prompt shows the path and the body; an overwrite shows what it replaces; a run prompt
shows the argv, the resolved binary and the directory; an output prompt shows the bytes and the
command that printed them; a vetting prompt shows the bytes, where they came from, and what the
check said about them rather than the word alone; the prompt about a directory a settings file
asked for shows the path it would open. A person cannot endorse a routing field they were not shown.

A vetting prompt about a picture ([VET-4](tools/vet-content.md#VET-4)) names a path for the person
to open the picture at rather than drawing it, because neither front end draws a picture. What is at
stake is still shown rather than summarised: the file at that path is a copy of the slot's bytes,
one step away, and [VET-4](tools/vet-content.md#VET-4) says why it is not the path they were read
from.

Every prompt a check was run for shows what it said, and the three are drawn out of one row builder
rather than three, so a prompt cannot carry a verdict and forget to say what it was. Which prompts
those are is [CHECK-10](vetting.md#CHECK-10).

The plan prompt shows the task in the person's own words and then every step, in order, each naming
its tier, what it would do, and every routing field the step fixes rather than the headline one
alone. Every step, never a count and never the first few: one answer covers all of them, and the
step below the fold is as binding as the first. A run prompt's arguments are the same: the last of
them runs as surely as the first, so no key that runs the line is taken until every row of its plan
has been on the screen ([PROMPT-4](#PROMPT-4)).

`verified-by: bravebot_tui::confirm::a_new_file_prompt_shows_the_path_and_body`
`verified-by: bravebot_tui::confirm::an_overwrite_prompt_shows_what_it_replaces`
`verified-by: bravebot_tui::confirm::a_run_prompt_shows_the_argv_the_binary_and_the_directory`
`verified-by: bravebot_tui::confirm::no_key_runs_a_long_plan_until_every_row_of_it_has_been_on_the_screen`
`verified-by: bravebot_tui::confirm::the_output_prompt_shows_the_bytes_and_the_command`
`verified-by: bravebot_tui::confirm::the_vet_prompt_shows_the_bytes_and_where_they_came_from`
`verified-by: bravebot_tui::confirm::the_output_prompt_says_what_a_check_found`
`verified-by: bravebot_tui::confirm::the_vouch_prompt_says_what_a_check_found`
`verified-by: bravebot_tui::trust_prompt::the_named_prompt_shows_the_directory_it_would_open`
`verified-by: bravebot_tui::confirm::the_plan_prompt_shows_the_task_and_every_step`
`verified-by: bravebot_core::manifest::a_described_step_names_every_routing_field_it_fixes`

<a id="PROMPT-2"></a>
### PROMPT-2: a prompt says what approving does, and what it does not

The run prompt says what the programs are held to: where the turn confines them
([SANDBOX-17](#SANDBOX-17)), the directories they are confined to and the toolchain lists and
credential scopes each stage brings beyond them ([SANDBOX-18](#SANDBOX-18)), and where it does not,
that they are not sandboxed. Which of the two it says is decided by the confinement the executor
starts the stages under, so it cannot disagree with what runs. On Linux and Windows, where no
`.git` is held back, the heading over those directories also says, for a line with a stage whose
program is not git and whose profile writes more than the temporary directory, that a program in
the line other than git can write a repository's hooks and configuration, which git runs later
with no sandbox ([SANDBOX-18](sandboxing.md#SANDBOX-18)); whether a stage's program is git is the
decision its profile is built with. It asks for the side effects and the
output together, and names the exact command it would vouch for. The output prompt says
what approving does. The vetting prompt says what approving does and, because nothing else on the
screen would, what it does not: no path is vouched for, so the same thing read again asks again. The trust prompt explains the consequence and names both answers. The prompt
about a directory a settings file asked for says that opening it grants reach and trust, and that a
file asked for it. The plan prompt says that nothing the run reads can add a step, drop one, or send
anything anywhere the plan does not already name; that approving the plan is not approving its
writes, which are each still put to the person as they come up; and that nothing has happened yet, so
declining leaves everything as it is.

**Why.** The second half of a run grant, that what the command prints becomes trusted, is the one
nothing else would tell the user. A hook or a configuration that a program other than git writes
into a `.git` where the platform cannot refuse the write runs with the person's whole access after
the line has ended, and nothing else tells them that either. A directory nobody typed is the same problem in the other
direction: without the question saying where the name came from, the box is unexplained. A plan is
the same problem at its largest, since a reader has no way to tell from a step list how much of the
run one keypress settles.

`verified-by: bravebot_tui::confirm::a_run_prompt_says_it_is_not_sandboxed_where_the_turn_does_not_confine`
`verified-by: bravebot_tui::confirm::a_run_prompt_says_what_a_confined_run_is_held_to`
`verified-by: bravebot_tui::confirm::a_run_prompt_says_a_program_other_than_git_can_write_a_repositorys_hooks`
`verified-by: bravebot_cli::plain::a_run_is_asked_about_as_confined_only_where_the_turn_confines`
`verified-by: bravebot_agent::confine::what_a_prompt_says_a_stage_carries_is_what_its_profile_grants`
`verified-by: bravebot_agent::confine::a_prompt_says_a_program_other_than_git_can_write_a_git_directory_where_none_is_held_back`
`verified-by: bravebot_agent::confirm::a_line_that_can_write_a_repositorys_hooks_says_so_in_its_heading`
`verified-by: bravebot_agent::turn::a_run_prompt_describes_the_confinement_only_where_the_turn_confines`
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
The questions a session opens with keep their question and their keys on screen the same way, with
what lies between scrolled by the keys the write and run prompts use (Up, Down, `k`, `j`, PageUp, PageDown, Home and
End, as [SCROLL-3](scroller.md#SCROLL-3) sets out), a page moving one row fewer
than the box shows, and none of them takes a yes at a draw that cuts either off.
A run prompt keeps its question and its keys on screen the same way and pages the same way. While a
row of its plan, down to the access the line spends, has not been on the screen during this
question, the row above the keys says how many have not, and `y`, `a` and `r` are not taken: each
runs the line. Reaching the bottom is not reading what was passed over, and a row read at one width
is not a row of the plan wrapped to another. What `a` and `r` grant besides is said below the plan,
and each of them also waits until the rows saying what it grants have been on the screen.
In the full-screen interface, the write, output, vetting, vouch, exposure, server, tool list, call
and plan prompts take no key that approves until every row that decides the question has been drawn
whole at the box's width, in one draw or over several as the body scrolls. What decides it is what
the prompt says about the content it shows and that content's first row; for a call it is the tool
and every argument, and for the exposure, server, tool list and plan prompts and a vetted picture it
is all of it, the picture's rows counting only at a draw that paints it whole. Until
then the row under the body says how many of those rows are left, and a draw at another width
starts the count again. A page moves one row fewer than the body shows, a box with no row for the
body beside keys drawn whole takes no yes, and a refusal and the key that stops the turn are taken
at every draw.
A processor's remark is bounded in **drawn rows**, so on a terminal of 80x24 or more a claim cannot
push the bytes it is a claim about below the fold, and the block says how many lines it is not
showing. On a smaller one the yes waits until those bytes have been drawn.

In the desktop window, a card that asks for an approval takes none until each row that decides it
has been on screen whole, at one moment or over several as the card is scrolled: inside every box
that clips it, in a window that is showing, with nothing drawn over it, an overlay the pointer
passes through included. The rows are the ones the terminal waits on for the same question. For a
write, reading output, a vetting card and a vouch they are the head and every row above the content,
and the content's first row; for a vetted picture they are all of it, down to the path to open. For
a run they are the head and every row of the plan, down to the access the line spends and each
warning, and a remembered approval also waits on the row saying what it covers. For a call they are
the tool and every argument, and not the server's description of the tool. For a language server,
an MCP server, a tool list, an exposure and a plan they are all of the card. The terminal keeps the
host of a fetch and of a moved server pinned, which a card that scrolls with the transcript cannot,
so the card waits on them instead: for a fetch, the first row of the address, the host and the
access it spends; for a moved server, where it was declared, the first row of where its reply
points, the host it reaches and the warning. A first row is the whole first line an element draws,
whatever it is made of. What a card folds under Details, and the fixed sentence the window closes a
card with, are not counted. The planner's own questions approve nothing and wait on nothing. Until
the rows have been on screen the row of answers says how many are left and an approval refuses a
press, and a refusal is taken at once. Nothing counts before the card is first measured, a press is
measured again when it lands, and a change in the card's width, or in how many rows an element
draws, starts the count again.

**Why.** Reviewing a whole file body on a terminal is not review, which is why `edit_file` exists
on a passage rather than a whole body. A prompt that scrolled the question away would be collecting a keypress, not a decision. A card
in a window is the same question asked with a mouse, so it owes the same.

`verified-by: bravebot_tui::confirm::a_long_body_keeps_the_question_on_screen_and_offers_the_rest`
`verified-by: bravebot_tui::confirm::the_rest_of_a_long_body_can_be_scrolled_to`
`verified-by: bravebot_tui::confirm::a_small_edit_in_a_large_file_shows_only_the_change`
`verified-by: bravebot_tui::confirm::output_that_is_empty_says_so`
`verified-by: bravebot_tui::confirm::vetted_content_that_is_empty_says_so`
`verified-by: bravebot_tui::confirm::a_preview_with_nothing_in_it_says_so`
`verified-by: bravebot_tui::confirm::an_uncomputable_diff_says_so`
`verified-by: bravebot_tui::confirm::a_long_plan_keeps_the_question_on_screen_and_offers_the_rest`
`verified-by: bravebot_tui::confirm::a_long_remark_does_not_push_the_diff_off_the_screen`
`verified-by: bravebot_tui::confirm::a_plan_longer_than_the_box_keeps_the_run_keys_and_says_how_many_rows_are_unread`
`verified-by: bravebot_tui::confirm::no_key_runs_a_long_plan_until_every_row_of_it_has_been_on_the_screen`
`verified-by: bravebot_tui::confirm::a_and_r_each_wait_for_the_rows_saying_what_they_grant_besides_running_the_line`
`verified-by: bravebot_tui::confirm::jumping_to_the_end_of_a_long_plan_leaves_the_rows_passed_over_unread`
`verified-by: bravebot_tui::confirm::a_run_prompt_too_small_for_its_question_takes_no_key_that_runs_the_line`
`verified-by: bravebot_tui::confirm::a_plan_read_at_one_width_is_unread_again_at_another`
`verified-by: bravebot_tui::confirm::paging_through_a_long_plan_in_a_short_box_puts_every_row_on_the_screen`
`verified-by: bravebot_tui::confirm::no_question_takes_a_yes_before_every_row_deciding_it_has_been_drawn`
`verified-by: bravebot_tui::confirm::a_box_with_no_room_for_the_body_beside_the_keys_takes_no_yes`
`verified-by: bravebot_tui::confirm::a_question_read_at_one_width_takes_no_yes_at_another`
`verified-by: bravebot_tui::confirm::a_question_says_how_many_rows_are_left_to_read_before_a_yes`
`verified-by: bravebot_tui::confirm::a_picture_the_box_cannot_paint_takes_no_yes`
`verified-by: bravebot_tui::confirm::a_tiny_terminal_draws_the_write_keys_and_takes_no_yes`
`verified-by: bravebot_tui::trust_prompt::a_list_longer_than_the_box_keeps_the_question_and_the_keys_and_says_how_many_rules_are_below`
`verified-by: bravebot_tui::trust_prompt::the_rest_of_a_long_list_can_be_scrolled_to`
`verified-by: bravebot_tui::trust_prompt::a_directory_question_longer_than_the_box_keeps_its_keys_and_scrolls_to_the_rest`
`verified-by: bravebot_tui::trust_prompt::the_arrows_scroll_the_question_and_answer_nothing`
`verified-by: bravebot_tui::trust_prompt::a_scroll_moves_from_where_the_box_was_drawn`
`verified-by: bravebot_tui::trust_prompt::paging_through_a_list_on_a_short_terminal_reads_every_rule`
`verified-by: bravebot_tui::trust_prompt::a_question_a_small_terminal_cuts_off_takes_no_answer`
`verified-by: bravebot_tui::trust_prompt::a_question_that_fits_its_box_says_nothing_of_scrolling`
`verified-by: by-construction (the desktop window is not a crate this workspace compiles; ui/scripts/shown.test.mjs pins that every row an approval rests on is marked as deciding for each kind of card and that the count opens, resets and stays shut as described, and make check-ui runs it; ui/scripts/drive-shown.mjs draws each card too tall for a 900x560 window and checks, independently of the card's own count, that no approval opens or is taken before those rows have been on screen)`

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
on the screen, and a person still has to press it with the bytes in front of them. At a prompt
about a picture ([VET-4](tools/vet-content.md#VET-4)), the bytes are a path away instead.

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

Saying no to a write does not stop the turn; Ctrl-C refuses it and does. **A question a session opens
with is answered by a key that arrived on its own, and by no other.** A terminal cannot say who wrote a
byte, but it can say what was waiting together ([INPUT-34](terminal-input.md#INPUT-34)), and a person
cannot fill the buffer between one read and the next: one event waiting is a keystroke and several are
a write. The line an editor types to activate a virtualenv spells an `n` on its way past, and the
question used to take it. What the question refuses is not dropped: the characters are carried to the
box, so somebody who finds a question still waiting and their virtualenv activated can read what did
it, rather than being left to join the two up themselves.

Leaving at such a question ends the session and opens nothing, and only Ctrl-C leaves. **It takes two
presses there**, and neither from a key that arrived with others: leaving before a session begins is the one
outcome nothing undoes, and an interrupt is a single byte another program can write into the terminal,
which an editor activating a virtualenv does ahead of the line it types
([INPUT-4](terminal-input.md#INPUT-4) keeps the same rule for the same reason). **The offer is
withdrawn by anything that is not that key**, a resize and words another program typed among them: the
interrupt an editor writes does arrive on its own and so arms it, and an offer left standing through
whatever happened next would let the next such byte take it, which is two presses turned back into
one. **Letting go of a key is not that**, since a release is the tail of the press being answered and
the way out of the first screen of a session cannot depend on the order somebody releases two keys.

**Why.** A refusal the agent can carry on past is how a person steers without starting over.

`verified-by: bravebot_tui::confirm::saying_no_does_not_stop_the_turn`
`verified-by: bravebot_tui::confirm::ctrl_c_refuses_the_write_and_stops_the_turn`
`verified-by: bravebot_tui::confirm::only_the_interrupt_stops_the_turn_at_a_run_prompt`
`verified-by: bravebot_tui::trust_prompt::ctrl_c_leaves_on_the_second_press`
`verified-by: bravebot_tui::trust_prompt::a_line_another_program_typed_answers_nothing`
`verified-by: bravebot_tui::trust_prompt::a_press_of_its_own_still_answers`
`verified-by: bravebot_tui::trust_prompt::what_the_question_refuses_is_carried_for_the_box`
`verified-by: bravebot_tui::trust_prompt::one_interrupt_another_program_wrote_closes_nothing`
`verified-by: bravebot_tui::trust_prompt::two_interrupts_that_arrived_together_close_nothing`
`verified-by: bravebot_tui::trust_prompt::anything_but_the_key_that_leaves_withdraws_the_offer`
`verified-by: bravebot_tui::trust_prompt::a_key_release_does_not_withdraw_the_offer`
`verified-by: bravebot_tui::trust_prompt::only_ctrl_c_leaves`
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

Errors and cancellation within a live session retain exact standing command approvals and live
advice. They do not turn one-use consent or per-policy refusals into standing permission. Saving
keeps standing approvals; resuming starts with no live advice or exposure answers.

`verified-by: bravebot_agent::turn::retention::ordinary_endings_retain_exact_programs_and_live_advice`
`verified-by: bravebot_cli::plain_retention_tests::failed_plain_turn_keeps_exact_approvals_advice_and_exposure`
`verified-by: bravebot_tui::undo_tests::ordinary_tui_endings_keep_exact_approvals_advice_and_exposure`
`verified-by: bravebot_ui_bridge::retention::worker::bridge_ordinary_endings_keep_decisions_live_and_resumed`

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

<a id="PROMPT-11"></a>
### PROMPT-11: every place an answer is kept is in the register, with what it is kept under

An answer is kept under a key, and a later effect whose key matches is let through without asking.
Every place that keeps one is a row of the table below. A row says which question fills it, what
that question showed, what the answer is kept under, and how long it lasts, and it cites the tests
that pin what it says. Every question the system asks lands in at least one row, and a question or a
place to keep answers that is new has its row before it lands.

**Kept under** is what a later effect has to match to go unasked. Anything **Shown** that the key does
not hold is something one answer covers in every version of it, so where a key holds less than its
prompt showed the row says what it leaves out.

The sets a session keeps past a prompt are the fields of `bravebot_agent::answers::Answers`, which
the terminal, the desktop bridge and a session in lines all hold. `/cd`, `/clear`, `/branch`, `/undo`
and `/rewind` in the terminal, a fork and an undo in the desktop bridge, and the end of a turn in the
bridge and in a session in lines each take it apart with no `..`, so a set added there does not
build until each of them says whether its answer is kept, forgotten or replaced. The terminal's turn
hands the sets back as the fields of `Continued`, which is checked against what a turn decides. An
answer a transition forgets is asked about again.

The paths programs were let reach and the hosts answered are held by the workspace, which `/clear`,
`/branch` and `/resume` empty of them. The planner's remembered answers are held by the terminal's
session, which `/clear` and `/branch` empty of them.

| Answer kept | Asked by | Shown | Kept under | Lasts | Evidence |
|---|---|---|---|---|---|
| one effect | a write or an edit, a run, reading what a program printed, being shown one quarantined slot, a fetch, the plan, a call to a server's tool, and removing a checkout something was done in | what each prompt shows ([PROMPT-1](#PROMPT-1)) | the exact value the approval was given for, by its own bytes ([PROMPT-5](#PROMPT-5)) | the one effect; it is never written down | `verified-by: bravebot_core::policy::an_endorsement_cannot_be_replayed` `verified-by: bravebot_core::policy::an_approval_for_one_url_does_not_fetch_another` `verified-by: bravebot_core::policy::an_approval_to_read_output_cannot_be_replayed` `verified-by: bravebot_core::policy::an_approval_to_vet_cannot_be_replayed` |
| the trust map | `y` at the startup question, at the question about a directory a settings file asked for, and at the offer to vouch for a quarantined file; `/add-dir` | the directory, or the file with a preview and what the check said | the path as text, made absolute, and not its bytes; a rule covers that path and every path below it, a whole segment at a time | the session; it is in the session record and comes back with `--resume` | `verified-by: bravebot_core::trust::a_rule_matches_whole_segments_only` `verified-by: bravebot_core::trust::one_added_directory_does_not_cover_a_sibling` `verified-by: bravebot_core::trust::a_yes_for_one_project_does_not_follow_a_move_to_another` `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` |
| a kept answer to the startup question | `y` at the startup question, where the question names the file it is written in | the directory, what a yes keeps, how far it reaches and the file | the directory's path and the identity the filesystem gives the directory there, so one made again at the same path is asked about | every later session started in that directory or below the root of its git worktree, until it is withdrawn | `verified-by: bravebot_agent::trusted::an_answer_kept_about_one_directory_answers_for_no_other` `verified-by: bravebot_agent::trusted::another_directory_at_the_same_path_is_asked_about` |
| granted rules | `y` at the question naming every allow rule a checkout's settings propose | each rule's text and the settings file it is in | the rule's text, the settings file's path and the root of the workspace it was granted in | a file every later session of the terminal reads in that workspace | `verified-by: bravebot_agent::granted::a_rule_edited_since_it_was_granted_is_not_granted` `verified-by: bravebot_agent::granted::a_grant_for_one_file_is_not_a_grant_for_another` `verified-by: bravebot_agent::granted::a_rule_granted_in_one_workspace_is_not_granted_in_another` |
| programs vouched for | `a` at the run prompt | each step's arguments, the binary it resolved to, and the directory | each step's resolved binary by its path's bytes, the name that started it, its exact arguments, and the directory, matched exactly. A line that sets a variable, writes through a redirection, feeds input from a reference, releases private data, asks for a credential scope or runs unconfined is not offered the key | the session; it is in the session record and comes back with `--resume` | `verified-by: bravebot_core::programs::vouching_in_one_tree_says_nothing_about_the_same_command_in_another` `verified-by: bravebot_core::programs::two_binaries_differing_only_in_unrenderable_bytes_are_different_programs` `verified-by: bravebot_core::policy::vouching_for_one_command_does_not_cover_another_of_the_same_program` `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` |
| lines remembered past the session | `r` and `f` at the run prompt | as for `a`, and the file the answer is recorded in | each step's name, resolved binary, the name that started it, every argument, its environment and where its streams go, how the steps are joined, and the directory. With `f`, the one number a table in the source lists is left free | a file every session of the terminal started in that directory reads, fresh or resumed | `verified-by: bravebot_core::remembered::no_entry_reaches_a_second_argument_list` `verified-by: bravebot_core::remembered::an_answer_does_not_follow_a_name_onto_a_different_binary` `verified-by: bravebot_core::remembered::an_environment_assignment_makes_a_different_line` `verified-by: bravebot_agent::remembered::a_line_answered_in_one_directory_does_not_answer_in_another` |
| run prompts drawn | every run prompt | the line | the line as it was asked about. It grants nothing: a later prompt for the same binary under other arguments reads it only to say that a settings file is what ends the asking | the session; it is written nowhere | `verified-by: bravebot_core::programs::asking_about_a_line_vouches_for_nothing` `verified-by: bravebot_core::policy::a_line_asked_about_is_not_thereby_vouched_for_or_remembered` `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` |
| reach kept from a run | `m` and `e` at the run prompt for a line that asked for a credential scope | the programs, the scopes, how long each lasts, and the file the record is written to | each program and operation word among the line's stages, with each scope, read only, bound to the checkout unless the scope table gives that program the scope. It vouches for no program, and the line is still asked about | `m`, the session; `e`, a file every later session in the checkout reads | `verified-by: bravebot_agent::reach::a_kept_request_binds_to_a_checkout_unless_the_table_gives_the_program_the_scope` `verified-by: bravebot_agent::turn::keeping_a_request_for_the_session_remembers_the_scope_for_that_session_only` |
| credential writes agreed to for the session | the standing key at a write prompt whose scan found a credential | the path, the body, and what the scan found | the file the write resolved to. It answers the scan's question only; the write is still asked about | the session; it is written nowhere | `verified-by: bravebot_agent::turn::always_for_a_credential_write_covers_that_file_and_no_other` `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` |
| credential writes remembered past the session | the recording key at the same prompt | as above | the file the write resolved to, in the record of the directory | a file every later session started in that directory reads | `verified-by: bravebot_agent::turn::remembering_a_credential_write_covers_later_sessions_in_that_directory_only` |
| files sent despite a credential | `y` at the question about a vouched file the scan found a credential in | the file's name, and each finding with its kind, where it is, and a masked value | the file's name from the root, or from an added directory's full path, as text. Not its bytes | the session; it is written nowhere and does not come back with `--resume` ([credential-protection.md](credential-protection.md#known-costs)) | `verified-by: bravebot_tui::app::an_answer_to_show_a_file_does_not_cover_the_file_by_that_name_after_moving` `verified-by: bravebot_agent::turn::agreeing_to_one_files_key_in_history_is_not_agreeing_to_anothers` `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` `verified-by: bravebot_ui_bridge::bridge::undoing_a_turn_stops_the_servers_and_keeps_the_files_agreed_to` `verified-by: bravebot_ui_bridge::bridge::a_desktop_fork_asks_again_about_what_only_the_parent_was_asked` |
| language servers started | `y` at the question about starting a language server | the program it resolved to, its arguments, and the tree it would index | the language, or a digest of a declared server's name, command, arguments, extensions, environment and options, for the root it was started in | the session, as a running process; a no is not kept | `verified-by: bravebot_agent::lsp::a_server_approved_in_one_turn_answers_the_next` `verified-by: bravebot_lsp::server::a_changed_declaration_is_asked_about_again` `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` `verified-by: bravebot_ui_bridge::bridge::undoing_a_turn_stops_the_servers_and_keeps_the_files_agreed_to` `verified-by: bravebot_ui_bridge::bridge::a_desktop_fork_asks_again_about_what_only_the_parent_was_asked` |
| MCP tool lists approved | the question about the tools a server offers | every tool, with its description | the server, for the session. A later session asks again, since nothing here may decide from the list's bytes that it is one already seen | the session, for a yes and for a no; it is written nowhere | `verified-by: bravebot_agent::mcp::a_list_is_asked_about_every_session_and_a_yes_records_nothing` `verified-by: bravebot_agent::mcp::a_declined_list_offers_nothing_and_is_not_asked_again` |
| standing answers to an MCP tool | answer 2 at a call to a server's tool | the tool and every argument | the server's alias, the tool, and the project. Not the arguments, which the answer leaves free, and not what the alias is declared as | a file every later session reads, until it is forgotten | `verified-by: bravebot_config::mcp::a_standing_answer_reaches_one_tool_of_one_server_in_one_project` |
| MCP server moves | `y` at the question about a remote server whose request was redirected | where the server was declared, where the reply points, the host it reaches, and the warning | the server's declaration, rewritten to the new destination only while it is still the declaration that was asked about | the settings file, for every later session; the session alone where the session writes nothing | `verified-by: bravebot_agent::mcp::a_move_is_recorded_over_the_declaration_asked_about_and_nothing_else` `verified-by: bravebot_agent::mcp::a_move_in_a_session_that_writes_nothing_lasts_for_the_session` |
| paths reached | `y` at the question about letting programs reach one more path | the path | the path it resolved to. A later request for a path already held is asked about like the first | the session; it is written nowhere | `verified-by: bravebot_agent::workspace::path_reach_is_numbered_shared_upgraded_and_ended` |
| hosts answered | the question about hosts programs asked for and were refused | the hosts | each host's name, for a yes and for a no | the session; it is written nowhere | `verified-by: bravebot_agent::tools::hosts_put_to_the_person_are_asked_once_and_the_answer_is_kept` `verified-by: bravebot_agent::workspace::host_answers_are_shared_remembered_and_lifted_by_a_yes` |
| answers to the planner's questions | a series of questions the planner asks | each question and its choices | every field of the question, each with its length, its tag and its choices included | the live session; it is written nowhere, and a resumed session asks again | `verified-by: bravebot_core::ask::no_question_can_spell_the_key_of_another` `verified-by: bravebot_tui::ask::a_series_answered_entirely_from_memory_asks_nothing` |
| auto-vetting | the standing key at a vetting or output prompt whose check completed and found nothing | the bytes, where they came from, and what the check said | nothing about any one slot. It turns on the mode in which a later slot whose check finds nothing reaches the planner unasked | a file every later session reads, fresh or resumed, until it is turned off | `verified-by: bravebot_tui::state::pressing_the_standing_key_turns_vetting_on_for_the_session` `verified-by: bravebot_tui::persist::a_recorded_answer_about_vetting_outlives_the_session_that_gave_it` |
| the permission mode | no question: the mode key, `/plan`, or starting a session that bypasses every check | the mode's name | nothing about any one effect. It answers in advance every prompt the mode reaches ([permission-modes.md](permission-modes.md)) | the sitting it was chosen in; it is not in the session record | `verified-by: bravebot_session::sessions::a_record_does_not_keep_a_permission_mode` `verified-by: bravebot_tui::state::cycling_the_mode_changes_nothing_a_resume_would_read` |
| the sandbox mode | `y` at the question `/sandbox off` asks; `/sandbox` naming any other mode asks nothing | that programs `run` starts will have no profile, so nothing confines what they read, write or reach, and the mode a no keeps | nothing about any one run. Every program a later turn starts runs with no profile | the process; `/resume` hands it to the session it starts | `verified-by: bravebot_tui::sandbox_command::a_move_to_off_asks_and_anything_but_a_yes_keeps_the_mode` `verified-by: bravebot_tui::sandbox_command::a_session_started_from_the_held_choice_keeps_the_commands_mode` |

The register above says what each answer is kept under. This table says what seven changes to the
world do to each answer, for every row of the register. A cell is one or more statements joined by
`;`, and a statement opens with what happens and says why:

- **asked again**: the same effect after the change is put to the person or refused, and a test shows
  it.
- **not reached**: the answer's key has no dimension the change touches, or the answer is not kept
  anywhere the change can carry it.
- **by design**: the answer holds, and the register or the clause cited says it does.
- **accepted**: the answer holds and a Known cost in the spec linked admits it.

**Moved** is `/cd` into a second checkout laid out like the first. **New session** is `/clear`,
`/branch`, a fork and `--resume`. **Subject replaced** is the file's bytes replaced, a link on the path
retargeted, a binary replaced in place or the branch switched. **Other directory** is the same name
under a sibling, a parent or a child. **Invocation changed** is an argument, the environment or a
redirection. **Declaration changed** is a server's alias, command or options. **More than shown** is
an answer covering more than the prompt displayed.

A cell citing `bravebot_agent::kept_answers::changing_the_world_under_a_yes_asks_again` is applied by
a case of that test's table, which `make check-security` holds to the cells citing it and the cells to
the cases.

| Answer kept | Moved | New session | Subject replaced | Other directory | Invocation changed | Declaration changed | More than shown |
|---|---|---|---|---|---|---|---|
| one effect | not reached: the endorsement is spent by the call it was given for, so no move falls between | not reached: it is never written down ([PROMPT-8](#PROMPT-8)) | asked again: an edit over a file that changed after the prompt is refused as stale, and a link repointed after the approval is refused before a step starts `verified-by: bravebot_agent::turn::a_stale_edit_is_refused` `verified-by: bravebot_agent::exec::a_link_pointed_elsewhere_after_approval_is_refused` | asked again: a plan's key opens with the directory it runs in `verified-by: bravebot_core::command::the_directory_a_plan_runs_in_is_keyed_on_its_bytes` `verified-by: bravebot_core::policy::an_endorsement_does_not_transfer_to_another_value` | asked again: the key holds every argument, the environment, where each stream goes and how the steps are joined `verified-by: bravebot_core::command::an_argument_cannot_forge_a_plans_encoding_boundary` `verified-by: bravebot_core::command::where_a_plan_writes_is_part_of_what_it_encodes` `verified-by: bravebot_core::command::an_environment_assignment_makes_a_different_plan` | not reached: a server's call is endorsed by its alias and tool for that one call | asked again: a key runs a long plan only once every row of it has been on the screen `verified-by: bravebot_tui::confirm::no_key_runs_a_long_plan_until_every_row_of_it_has_been_on_the_screen` |
| the trust map | asked again: a rule is a path made absolute, so a yes for one checkout does not reach the same names in another `verified-by: bravebot_core::trust::a_yes_for_one_project_does_not_follow_a_move_to_another`; by design: the move vouches for the directory moved to, because the person typed it ([TRUST-13](trust-map.md#TRUST-13)) | asked again: `/clear` asks the startup question afresh `verified-by: bravebot_tui::app::a_fresh_session_is_asked_rather_than_inheriting_a_map`; by design: `/branch`, a fork and `--resume` carry the map the record holds ([PROMPT-8](#PROMPT-8)) `verified-by: bravebot_tui::app::a_branch_keeps_what_the_record_holds_and_forgets_the_rest` | accepted: [trust-map.md](trust-map.md#known-costs) | asked again: a rule covers whole path segments, so a sibling or a parent is not reached `verified-by: bravebot_core::trust::a_rule_matches_whole_segments_only` `verified-by: bravebot_core::trust::one_added_directory_does_not_cover_a_sibling`; by design: a directory covers every path beneath it ([TRUST-2](trust-map.md#TRUST-2)) | not reached: the map holds paths and no command | not reached: the map names no server | by design: a rule covers its path and everything beneath it, and a vouch is offered for a file only ([TRUST-2](trust-map.md#TRUST-2)) `verified-by: bravebot_agent::turn::a_path_that_names_no_file_is_not_offered_for_vouching` |
| a kept answer to the startup question | not reached: `/cd` never reads the kept record, and the key is the directory's own path and identity ([TRUST-23](trust-map.md#TRUST-23)) `verified-by: bravebot_agent::trusted::an_answer_kept_about_one_directory_answers_for_no_other` | by design: a yes at the question is kept for every later session started there ([TRUST-23](trust-map.md#TRUST-23)) `verified-by: bravebot_agent::trusted::a_kept_answer_is_read_back_by_the_next_session_there` | asked again: a directory removed and made again has another identity `verified-by: bravebot_agent::trusted::another_directory_at_the_same_path_is_asked_about` `verified-by: bravebot_agent::trusted::a_directory_removed_and_made_again_has_another_identity`; by design: what is replaced inside the directory arrives with it ([TRUST-23](trust-map.md#TRUST-23)) | asked again: only the directory answered about, or the worktree root above it, is settled by it `verified-by: bravebot_agent::trusted::a_nested_repository_is_asked_about` `verified-by: bravebot_agent::trusted::only_a_worktree_root_answers_for_what_is_below_it` | not reached: the answer is about a directory and no command | not reached: the answer is about a directory and no server | asked again: `y` is taken only after the lines saying what it keeps and how far it reaches have been on the screen `verified-by: bravebot_tui::trust_prompt::y_trusts_nothing_until_the_record_it_keeps_has_been_on_the_screen` `verified-by: bravebot_tui::trust_prompt::the_prompt_that_keeps_a_yes_names_the_record_it_writes` |
| granted rules | asked again: an entry names the settings file's path and the workspace it was granted in `verified-by: bravebot_agent::granted::a_rule_granted_in_one_workspace_is_not_granted_in_another` `verified-by: bravebot_tui::app::a_rule_granted_in_one_checkout_is_not_in_force_after_moving_to_another` | by design: a grant outlives the session that gave it, in that workspace ([PERM-15](permissions.md#PERM-15)) `verified-by: bravebot_agent::granted::a_rule_granted_by_one_session_is_read_back_by_another` | asked again: a rule whose text was edited since it was granted is not granted `verified-by: bravebot_agent::granted::a_rule_edited_since_it_was_granted_is_not_granted`; accepted: [permissions.md](permissions.md#known-costs) | asked again: a sibling, a parent and a child are other workspaces, each with its own record `verified-by: bravebot_agent::granted::a_workspace_sharing_a_key_with_another_is_not_answered_by_its_lines` `verified-by: bravebot_agent::granted::a_grant_for_one_file_is_not_a_grant_for_another` | asked again: an assignment, a write redirection or private input asks whatever a rule allows `verified-by: bravebot_core::policy::an_environment_assignment_asks_even_for_a_line_a_rule_allows` `verified-by: bravebot_core::policy::private_input_asks_even_for_a_line_a_rule_allows` `verified-by: bravebot_core::policy::a_write_redirection_asks_even_for_a_line_a_rule_allows`; by design: the rule's pattern says which arguments it covers `verified-by: bravebot_core::permissions::a_command_pattern_matches_where_the_documented_table_says` | by design: a rule for a server's tool names its alias and tool, and the server's approval is a digest of its declaration ([SERVERS-5](mcp-servers.md#SERVERS-5)) | asked again: `y` grants nothing until every rule has been on the screen whole, with its file `verified-by: bravebot_tui::trust_prompt::y_grants_nothing_while_a_rule_has_not_been_on_the_screen` `verified-by: bravebot_tui::trust_prompt::a_rule_that_wraps_past_the_box_is_never_granted` |
| programs vouched for | asked again: an entry names the tree it was vouched in, compared by equality `verified-by: bravebot_core::programs::vouching_in_one_tree_says_nothing_about_the_same_command_in_another` `verified-by: bravebot_tui::app::moving_keeps_the_programs_vouched_for_and_the_prompts_already_drawn` | asked again: `/clear` vouches for nothing `verified-by: bravebot_tui::app::clearing_forgets_every_answer_the_session_it_ends_gave`; by design: `/branch`, a fork and `--resume` carry the list the record holds ([RUN-9](tools/run.md#RUN-9)) `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` | asked again: a name that resolves to another binary, or a link repointed after the approval, is another entry `verified-by: bravebot_core::policy::vouching_does_not_follow_a_name_onto_a_different_binary` `verified-by: bravebot_agent::exec::a_link_pointed_elsewhere_after_approval_is_refused`; by design: a binary or a script replaced in place at the same path is covered, as it is in the record that outlasts the session ([RUN-19](tools/run.md#RUN-19)) | asked again: the tree is compared by equality, so a directory below or above is another grant `verified-by: bravebot_core::programs::an_entry_does_not_cover_a_directory_below_the_one_it_names` `verified-by: bravebot_core::programs::the_same_name_at_a_different_path_is_a_different_program` | asked again: the arguments must match exactly, and an assignment, a write redirection or private input asks first `verified-by: bravebot_core::programs::the_arguments_must_match_exactly` `verified-by: bravebot_core::policy::a_vouched_line_carrying_an_environment_assignment_is_asked_about` `verified-by: bravebot_core::policy::a_line_that_writes_is_asked_about_even_when_its_steps_are_vouched_for`; by design: a step's merged streams are not in the key, and the session bounds that ([RUN-19](tools/run.md#RUN-19)) | not reached: the key is a program line and names no server | by design: a line runs unasked when every step in it was vouched for, however the steps are joined ([RUN-8](tools/run.md#RUN-8)); asked again: `a` is taken only after the rows saying what it grants have been shown `verified-by: bravebot_tui::confirm::a_and_r_each_wait_for_the_rows_saying_what_they_grant_besides_running_the_line` |
| lines remembered past the session | asked again: the record is read for the directory the session is in, and an entry names its directory `verified-by: bravebot_agent::remembered::a_line_answered_in_one_directory_does_not_answer_in_another` `verified-by: bravebot_core::policy::a_remembered_line_run_outside_the_root_is_asked_about_anyway` | by design: the record is a file every session started in that directory reads, fresh or resumed ([RUN-19](tools/run.md#RUN-19)) `verified-by: bravebot_agent::remembered::a_line_written_by_one_session_is_read_back_by_another` | asked again: another binary behind the name, or another path it is started by, is another line `verified-by: bravebot_core::remembered::an_answer_does_not_follow_a_name_onto_a_different_binary`; by design: a binary replaced in place, a file the line takes its work from and a directory removed and made again at the same path are covered, which is what an answer that lasts costs ([RUN-19](tools/run.md#RUN-19)) | asked again: an entry names its directory, and a line run outside the root is asked about anyway `verified-by: bravebot_agent::remembered::a_directory_sharing_a_key_with_another_is_not_answered_by_its_lines` `verified-by: bravebot_core::policy::a_remembered_line_run_outside_the_root_is_asked_about_anyway` | asked again: every argument, the environment, where the streams go and how the steps are joined are in the key `verified-by: bravebot_core::remembered::no_entry_reaches_a_second_argument_list` `verified-by: bravebot_core::remembered::an_environment_assignment_makes_a_different_line` `verified-by: bravebot_core::remembered::sending_the_streams_somewhere_else_makes_a_different_line`; by design: `f` leaves one whole number free, in the shapes a table in the source lists ([RUN-20](tools/run.md#RUN-20)) | not reached: the key is a program line and names no server | by design: `f` frees one whole number and the prompt draws it as `<number>` ([RUN-20](tools/run.md#RUN-20)) `verified-by: bravebot_core::policy::a_family_remembered_past_the_session_is_not_asked_about_for_another_number` |
| run prompts drawn | not reached: it grants nothing, and a move leaves what it only advises with | asked again: `/clear`, `/branch` and a fork forget it, and a resume builds it empty `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` | not reached: it grants nothing, so a replaced binary is only the same call | not reached: it grants nothing, and the directory is ignored on purpose when it advises | not reached: it grants nothing, and a run prompt is drawn for every line whatever its arguments | not reached: it grants nothing and names no server | not reached: it grants nothing |
| reach kept from a run | asked again: a grant is bound to the checkout's root and identity unless the scope table names the program `verified-by: bravebot_agent::reach::a_kept_request_binds_to_a_checkout_unless_the_table_gives_the_program_the_scope` `verified-by: bravebot_agent::reach::a_directory_row_is_in_force_only_in_the_checkout_it_was_typed_in`; by design: a program the scope table names has the scope in every checkout, and the prompt says so ([SANDBOX-23](sandboxing.md#SANDBOX-23)) `verified-by: bravebot_agent::reach::a_scope_for_a_program_the_table_names_is_in_force_in_every_checkout` | asked again: `m` is kept for the session's id, which `/clear`, `/branch` and a fork replace `verified-by: bravebot_agent::turn::keeping_a_request_for_the_session_remembers_the_scope_for_that_session_only`; by design: `e` is a file every later session in the checkout reads `verified-by: bravebot_agent::turn::keeping_a_request_for_every_session_is_read_by_another_session` | by design: the key is the program's path, and the checkout is the unit of the grant ([SANDBOX-23](sandboxing.md#SANDBOX-23)) | asked again: another path for the binary is another key `verified-by: bravebot_agent::reach::a_grant_covers_the_file_and_the_operation_it_was_made_for` | asked again: an option before the operation or an assignment removes the grant `verified-by: bravebot_agent::reach::an_assignment_in_front_of_a_step_removes_every_grant` `verified-by: bravebot_agent::reach::a_command_that_starts_with_an_option_carries_no_grant`; by design: the arguments after the operation word are free ([SANDBOX-23](sandboxing.md#SANDBOX-23)) | not reached: the key is a program and an operation and names no server | by design: the prompt names the shapes and how long each lasts, and the key is the shape ([SANDBOX-23](sandboxing.md#SANDBOX-23)) |
| credential writes agreed to for the session | asked again: `/cd` drops the set, and the key is an absolute path `verified-by: bravebot_tui::credential_mode_tests::always_for_a_credential_does_not_follow_the_session_through_cd` | asked again: `/clear`, `/branch` and a fork forget it `verified-by: bravebot_tui::credential_mode_tests::always_for_a_credential_does_not_outlive_clear` `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` | asked again: a link retargeted on the path resolves to another file `verified-by: bravebot_agent::kept_answers::changing_the_world_under_a_yes_asks_again`; accepted: [credential-protection.md](credential-protection.md#known-costs) | asked again: the key is the resolved path, compared exactly `verified-by: bravebot_agent::turn::always_for_a_credential_write_covers_that_file_and_no_other` | not reached: a write names a path and carries no command, argument or redirection | not reached: the key is a path and names no server | by design: the answer covers the file whatever the next body holds, and the write is still asked about ([CRED-15](credential-protection.md#CRED-15)) `verified-by: bravebot_agent::turn::always_for_an_edit_that_leaves_a_credential_covers_the_next_edit_to_that_file` |
| credential writes remembered past the session | asked again: an entry names the directory and the file, and the record is read for the directory the session is in `verified-by: bravebot_agent::turn::remembering_a_credential_write_covers_later_sessions_in_that_directory_only` `verified-by: bravebot_agent::remembered::a_file_written_by_one_session_is_read_back_by_another_in_that_directory_only` | by design: the record is read by every session started in that directory ([CRED-13](credential-protection.md#CRED-13)) | asked again: a link retargeted on the path resolves to another file `verified-by: bravebot_agent::kept_answers::changing_the_world_under_a_yes_asks_again`; accepted: [credential-protection.md](credential-protection.md#known-costs) | asked again: an entry covers one file of one directory `verified-by: bravebot_core::remembered::a_file_entry_covers_that_file_only` `verified-by: bravebot_agent::remembered::a_file_outside_the_directory_is_neither_written_nor_read` | not reached: a write names a path and carries no command, argument or redirection | not reached: the key is a path and names no server | by design: the entry stays until its line is deleted ([CRED-13](credential-protection.md#CRED-13)) |
| files sent despite a credential | asked again: `/cd` drops the set, since the key is a name from the root `verified-by: bravebot_tui::app::an_answer_to_show_a_file_does_not_cover_the_file_by_that_name_after_moving` | asked again: `/clear`, `/branch` and a fork forget it, and `--resume` starts without it `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` `verified-by: bravebot_ui_bridge::bridge::a_desktop_fork_asks_again_about_what_only_the_parent_was_asked` | accepted: [credential-protection.md](credential-protection.md#known-costs) | asked again: the key carries the path from the root, so `sub/.env` is not `.env` `verified-by: bravebot_agent::kept_answers::changing_the_world_under_a_yes_asks_again` | by design: the answer covers the file whichever lines are read, an edit shows included ([CRED-15](credential-protection.md#CRED-15)) `verified-by: bravebot_agent::turn::an_agreement_to_show_a_file_covers_the_lines_an_edit_shows` | not reached: the key is a name and names no server | by design: the scan covers the window read and the answer covers the whole file ([CRED-15](credential-protection.md#CRED-15)) `verified-by: bravebot_agent::turn::a_file_agreed_to_once_is_not_asked_about_again_this_session` |
| language servers started | asked again: `/cd` stops the servers started in the old directory `verified-by: bravebot_tui::app::changing_directory_stops_the_servers_started_in_the_old_one` | asked again: `/clear`, `/branch` and a fork forget the servers `verified-by: bravebot_agent::answers::every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again` `verified-by: bravebot_ui_bridge::bridge::undoing_a_turn_stops_the_servers_and_keeps_the_files_agreed_to` | not reached: the approval is a running process, and a program replaced afterwards is not started by it | not reached: a server runs for the root it was started in, and `/cd` leaves it ([LSP-5](tools/lsp.md#LSP-5)) | not reached: the program and arguments come from a fixed table or from the declaration | asked again: the digest covers the name, command, arguments, extensions, environment and options `verified-by: bravebot_lsp::server::a_changed_declaration_is_asked_about_again` `verified-by: bravebot_agent::declared_lsp::a_changed_declaration_is_asked_about_again` | by design: the language covers every file and operation, and the prompt says the server reads the tree and may run build tooling ([LSP-8](tools/lsp.md#LSP-8)) `verified-by: bravebot_lsp::server::the_prompt_says_which_servers_run_build_tooling` |
| MCP tool lists approved | not reached: the digest names a declaration and no checkout | by design: a yes lives in a file every later session reads, and a no is kept by the MCP session for the process ([SERVERS-4](mcp-servers.md#SERVERS-4)) `verified-by: bravebot_agent::mcp::a_declined_list_offers_nothing_and_is_not_asked_again` | asked again: a list that differs from the one drawn is asked about again `verified-by: bravebot_agent::mcp::a_list_vouched_for_before_asks_nothing_and_a_changed_one_asks_again`; by design: a program replaced in place that draws the same list is covered, which is all a digest can cover ([SERVERS-5](mcp-servers.md#SERVERS-5)) | asked again: the digest covers the program, every argument and the directory `verified-by: bravebot_config::mcp::a_digest_covers_the_program_every_argument_the_names_and_the_directory` | asked again: the digest covers every stored value and every file the server may read `verified-by: bravebot_config::mcp::a_digest_covers_every_stored_value_and_every_file_it_may_read`; by design: the value of a variable read from the environment is not in the declaration ([SERVERS-5](mcp-servers.md#SERVERS-5)) | asked again: a changed declaration has a digest nothing approved `verified-by: bravebot_config::mcp::a_vouched_list_lasts_as_long_as_its_declaration_resolves`; by design: an alias is a label and approves nothing, so renaming one keeps the vouch ([SERVERS-5](mcp-servers.md#SERVERS-5)) `verified-by: bravebot_config::mcp::a_digest_is_the_same_whatever_the_alias_or_the_order_of_the_names` | asked again: the key is a digest of the list as drawn, and the prompt takes no yes until every row is on the screen `verified-by: bravebot_tui::confirm::no_question_takes_a_yes_before_every_row_deciding_it_has_been_drawn` `verified-by: bravebot_tui::confirm::a_tool_list_draws_every_description_row_behind_the_margin` |
| standing answers to an MCP tool | asked again: the project is part of the key and follows the session `verified-by: bravebot_agent::mcp::answer_two_follows_the_session_to_another_project` `verified-by: bravebot_config::mcp::a_standing_answer_reaches_one_tool_of_one_server_in_one_project` | by design: the answer is a file read on every call, until it is forgotten ([SERVERS-4](mcp-servers.md#SERVERS-4)) `verified-by: bravebot_cli::mcp::forget_drops_a_projects_standing_answers_and_nobody_elses` | accepted: [mcp-servers.md](mcp-servers.md#known-costs) | asked again: the key is the alias, the tool and the project, each compared exactly `verified-by: bravebot_agent::mcp::answer_two_stops_asking_for_the_one_tool_in_the_one_project` | by design: the arguments are free, and a call carrying the person's private data asks whatever stands ([SERVERS-4](mcp-servers.md#SERVERS-4)) `verified-by: bravebot_core::policy::private_arguments_ask_even_for_a_tool_a_rule_allows` | accepted: [mcp-servers.md](mcp-servers.md#known-costs) | by design: the prompt shows one call and the answer covers the tool's later calls in the project, whatever their arguments ([SERVERS-4](mcp-servers.md#SERVERS-4)) `verified-by: bravebot_tui::confirm::a_call_prompt_draws_the_tool_its_arguments_and_three_answers` |
| MCP server moves | not reached: a declaration is global, and the move rewrites it | by design: the rewritten declaration is read by every session, and in a session that writes nothing it lasts for the process ([SERVERS-11](mcp-servers.md#SERVERS-11)) `verified-by: bravebot_agent::mcp::a_move_in_a_session_that_writes_nothing_lasts_for_the_session` | asked again: the declaration is rewritten only while its digest is still the one asked about `verified-by: bravebot_agent::mcp::a_move_is_recorded_over_the_declaration_asked_about_and_nothing_else` | not reached: the hop gate compares a host and a port and no path | by design: after a yes every later request goes to the new destination, whatever the tool or arguments ([SERVERS-11](mcp-servers.md#SERVERS-11)) `verified-by: bravebot_agent::mcp::a_hop_the_person_says_is_a_move_is_declared_and_the_call_reaches_it` | asked again: a declaration edited since the question is left as it is `verified-by: bravebot_agent::mcp::a_move_is_recorded_over_the_declaration_asked_about_and_nothing_else` | by design: the yes re-declares the server for every project and session, and the prompt says so ([SERVERS-11](mcp-servers.md#SERVERS-11)) |
| paths reached | by design: the path is absolute and the grant is the session's, so a move keeps it ([SANDBOX-28](sandboxing.md#SANDBOX-28)) | asked again: `/clear`, `/branch` and `/resume` end it, and a new process starts without it `verified-by: bravebot_agent::workspace::a_session_ending_in_this_process_takes_its_reach_and_host_answers_with_it` `verified-by: bravebot_agent::workspace::a_new_workspace_holds_no_path_reach` `verified-by: bravebot_tui::app::a_branch_keeps_what_the_record_holds_and_forgets_the_rest` | by design: the grant is the path as it resolved when the person answered, and bytes replaced at it are covered ([SANDBOX-28](sandboxing.md#SANDBOX-28)) | asked again: a parent, a sibling and a directory sharing the granted name's prefix get no row `verified-by: bravebot_agent::confine::a_path_let_reach_does_not_reach_its_parent_or_a_sibling`; by design: a directory covers what is beneath it ([SANDBOX-28](sandboxing.md#SANDBOX-28)) | by design: the grant is for every later `run`, whichever program it starts ([SANDBOX-28](sandboxing.md#SANDBOX-28)) `verified-by: bravebot_agent::turn::a_yes_to_a_path_lets_a_program_write_it_and_a_read_only_yes_does_not` | not reached: the key is a path and names no server | by design: a yes to a directory is a yes to what is beneath it, and the question shows the path as it resolves ([SANDBOX-28](sandboxing.md#SANDBOX-28)) `verified-by: bravebot_tui::confirm::a_path_longer_than_the_box_takes_no_yes_until_the_end_of_it_has_been_drawn` |
| hosts answered | by design: a host is a name, and the answer is the session's ([SANDBOX-24](sandboxing.md#SANDBOX-24)) | asked again: `/clear`, `/branch` and `/resume` end it, and a new process starts without it `verified-by: bravebot_agent::workspace::a_session_ending_in_this_process_takes_its_reach_and_host_answers_with_it` `verified-by: bravebot_agent::workspace::a_new_workspace_holds_no_host_answers` `verified-by: bravebot_tui::app::a_branch_keeps_what_the_record_holds_and_forgets_the_rest` | by design: the proxy decides on the name, and a granted name is held to the addresses it resolves to ([SANDBOX-24](sandboxing.md#SANDBOX-24)) `verified-by: bravebot_agent::confine::a_granted_host_changes_the_list_the_next_line_is_held_to` | asked again: the answer is the exact name, so a subdomain is another host `verified-by: bravebot_agent::kept_answers::changing_the_world_under_a_yes_asks_again` | by design: a yes adds the host to the allowed set of every stage from the next line ([SANDBOX-24](sandboxing.md#SANDBOX-24)) `verified-by: bravebot_agent::tools::hosts_put_to_the_person_are_asked_once_and_the_answer_is_kept` | not reached: a host answer names no server, and a denied entry still wins ([SANDBOX-24](sandboxing.md#SANDBOX-24)) | asked again: the question names up to eight hosts, and a yes grants only those named `verified-by: bravebot_agent::tools::hosts_put_to_the_person_are_asked_once_and_the_answer_is_kept` |
| answers to the planner's questions | by design: the conversation goes on across `/cd`, so a repeated question is the same exchange ([ASK-8](tools/ask-user.md#ASK-8)) | asked again: `/clear` and `/branch` forget them, and a resumed session starts without them `verified-by: bravebot_tui::state::clearing_the_conversation_forgets_the_answers_to_the_planners_questions` `verified-by: bravebot_tui::app::a_branch_keeps_what_the_record_holds_and_forgets_the_rest` | not reached: the key is the text of the question, and the answer is the person's reply, which grants nothing | not reached: the key is the text of the question and names no directory | asked again: the key holds every field of the question, its tag and its choices `verified-by: bravebot_core::ask::the_key_distinguishes_questions_that_differ_anywhere` | not reached: the key is the text of the question and names no server | asked again: the key holds more than the screen draws, the tag, the length and the choices `verified-by: bravebot_core::ask::the_key_distinguishes_questions_that_differ_in_their_tag` `verified-by: bravebot_core::ask::no_question_can_spell_the_key_of_another` |
| auto-vetting | by design: it is a mode of the session, set by the home layer alone, and names no checkout ([CHECK-11](vetting.md#CHECK-11)) | by design: it is a file every later session reads, fresh or resumed `verified-by: bravebot_tui::persist::a_recorded_answer_about_vetting_outlives_the_session_that_gave_it`; asked again: an incognito session does not read it `verified-by: bravebot_tui::incognito::the_standing_answer_about_vetting_is_not_read` | asked again: only a check that completed and found nothing answers for a slot `verified-by: bravebot_agent::turn::with_auto_vetting_an_unsafe_verdict_still_asks` `verified-by: bravebot_agent::turn::with_auto_vetting_a_check_that_could_not_be_made_still_asks` | not reached: it holds no key and names no directory | not reached: it holds no key and names no command | asked again: it never answers the vouch offer or a server's tool list `verified-by: bravebot_agent::turn::auto_vetting_does_not_answer_the_vouch_offer` | by design: the prompt shows one slot and the answer covers every later slot whose check finds nothing ([CHECK-11](vetting.md#CHECK-11)) `verified-by: bravebot_tui::confirm::only_a_safe_verdict_offers_to_stop_asking` |
| the permission mode | by design: the mode is the sitting's, and a checkout that forbids bypassing takes it away ([MODE-10](permission-modes.md#MODE-10)) `verified-by: bravebot_tui::app::reading_the_rules_of_a_checkout_that_forbids_bypassing_takes_bypass_away` | asked again: `--resume`, `/resume` and a desktop open or fork start asking `verified-by: bravebot_session::sessions::a_record_does_not_keep_a_permission_mode` `verified-by: bravebot_ui_bridge::permission_mode::every_way_of_opening_a_session_opens_asking`; by design: `/clear` and `/branch` go on in the same sitting ([MODE-10](permission-modes.md#MODE-10)) | not reached: the mode holds no key | not reached: the mode holds no key | not reached: the mode holds no key | not reached: the mode holds no key | by design: it answers in advance every prompt the mode reaches ([MODE-10](permission-modes.md#MODE-10)) `verified-by: bravebot_tui::state::cycling_the_mode_changes_nothing_a_resume_would_read` |
| the sandbox mode | by design: the mode lasts the process, and `/cd` does not read it again ([SANDBOX-22](sandboxing.md#SANDBOX-22)) | by design: `/clear`, `/resume` and `/branch` keep it, and a new process chooses at its start ([SANDBOX-22](sandboxing.md#SANDBOX-22)) `verified-by: bravebot_tui::sandbox_command::a_session_started_from_the_held_choice_keeps_the_commands_mode` | not reached: the mode holds no key | not reached: the mode holds no key | not reached: the mode holds no key | not reached: the mode holds no key | by design: the question says no profile applies to any program, and Enter keeps the mode ([SANDBOX-22](sandboxing.md#SANDBOX-22)) `verified-by: bravebot_tui::sandbox_command::a_move_to_off_asks_and_anything_but_a_yes_keeps_the_mode` |

**Why.** A yes about one thing comes to cover another when the key it is kept under holds less than
the prompt showed, or is relative to something that later moves. With no row written before a
question lands, nothing makes a prompt state what its answer is kept under, and a key that holds
less than the prompt showed lets one yes cover another effect. Writing the row first decides what
the key holds where a reviewer reads it, and the audit can hold the code to it.

<a id="PROMPT-9"></a>
### PROMPT-9: where nobody can be asked, the answer is no

A one-shot run refuses effects rather than applying them unseen, and declines every question
rather than inventing an answer. A closed channel refuses a run and answers no
question, and it refuses to promote a slot however the check that read it answered: a word from a
model is not a person having read something. A plan is refused too, so a manifest run with nobody to ask stops before its first step
rather than walking a program nobody read.

A person may answer these prompts in advance, for a session or for a run, by choosing a mode:
[permission-modes.md](permission-modes.md) is what each mode answers and what asking for one costs.
That is somebody's own standing answer rather than a default, and no mode answers a question the
planner posed, since that asks for information rather than consent.

`--vet` ([vetting.md](vetting.md#CHECK-11)) is such an answer in advance, and to one of them
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
